#!/usr/bin/env python3
"""Normalize a white-background pet sheet into a Codux 8x9 atlas.

Pipeline (single flow, no fallbacks):
1. Strip edge-connected white background on the whole source image. Internal
   white (white fur, white belly, highlights) is never touched because it
   never reaches the outer image boundary.
2. Find connected components of non-transparent pixels and drop tiny noise.
3. Cluster components into row bands by centroid Y (gap-based, adapts to AI
   layouts that don't sit on the strict pixel grid).
4. Inside each row band, cluster by centroid X to merge body+tail+highlight
   pieces of the same character into one frame.
5. Each frame is fit-scaled into 192x208 with a bottom baseline and pasted
   into a 1536x1872 transparent atlas. Empty rows/cells stay transparent.

Output:
- <output_dir>/spritesheet.png
- <output_dir>/pet.json
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

try:
    from PIL import Image
    import numpy as np
except ModuleNotFoundError as exc:
    print(
        f"Missing Python dependency: {exc.name}. Install it with `python3 -m pip install Pillow numpy`.",
        file=sys.stderr,
    )
    raise SystemExit(1)

sys.path.insert(0, str(Path(__file__).resolve().parent))
from bg_remove import remove_white_bg


COLUMNS = 8
ROWS = 9
CELL_WIDTH = 192
CELL_HEIGHT = 208
ATLAS_WIDTH = COLUMNS * CELL_WIDTH    # 1536
ATLAS_HEIGHT = ROWS * CELL_HEIGHT     # 1872


def find_components(rgba: Image.Image, alpha_threshold: int, min_area: int) -> list[dict]:
    """Iterative 4-connectivity BFS over the alpha mask. Returns one dict per
    component with bbox (left, top, right, bottom), centroid (x, y), area."""
    width, height = rgba.size
    alpha_bytes = rgba.split()[3].tobytes()
    visited = bytearray(width * height)
    components: list[dict] = []

    for sy in range(height):
        row_offset = sy * width
        for sx in range(width):
            idx = row_offset + sx
            if visited[idx] or alpha_bytes[idx] <= alpha_threshold:
                continue

            stack = [idx]
            visited[idx] = 1
            count = 0
            sum_x = 0
            sum_y = 0
            min_x = max_x = sx
            min_y = max_y = sy

            while stack:
                cidx = stack.pop()
                cy = cidx // width
                cx = cidx - cy * width
                count += 1
                sum_x += cx
                sum_y += cy
                if cx < min_x:
                    min_x = cx
                if cx > max_x:
                    max_x = cx
                if cy < min_y:
                    min_y = cy
                if cy > max_y:
                    max_y = cy

                if cx > 0:
                    n = cidx - 1
                    if not visited[n] and alpha_bytes[n] > alpha_threshold:
                        visited[n] = 1
                        stack.append(n)
                if cx + 1 < width:
                    n = cidx + 1
                    if not visited[n] and alpha_bytes[n] > alpha_threshold:
                        visited[n] = 1
                        stack.append(n)
                if cy > 0:
                    n = cidx - width
                    if not visited[n] and alpha_bytes[n] > alpha_threshold:
                        visited[n] = 1
                        stack.append(n)
                if cy + 1 < height:
                    n = cidx + width
                    if not visited[n] and alpha_bytes[n] > alpha_threshold:
                        visited[n] = 1
                        stack.append(n)

            if count >= min_area:
                components.append({
                    "bbox": (min_x, min_y, max_x + 1, max_y + 1),
                    "centroid": (sum_x / count, sum_y / count),
                    "area": count,
                })

    return components


def cluster_by_axis_gap(components: list[dict], axis: int, gap_threshold: float) -> list[list[dict]]:
    """Group components into clusters along one axis (0=x, 1=y). Two adjacent
    components in the sorted list start a new cluster when their centroid
    distance along the axis exceeds gap_threshold."""
    if not components:
        return []
    sorted_components = sorted(components, key=lambda c: c["centroid"][axis])
    clusters: list[list[dict]] = [[sorted_components[0]]]
    for prev, curr in zip(sorted_components, sorted_components[1:]):
        if curr["centroid"][axis] - prev["centroid"][axis] > gap_threshold:
            clusters.append([curr])
        else:
            clusters[-1].append(curr)
    return clusters


def assign_to_cells(
    components: list[dict],
    source_w: int,
    source_h: int,
    row_gap_ratio: float,
    column_gap_ratio: float,
) -> dict[tuple[int, int], list[dict]]:
    """Two-stage gap clustering:
    1. Cluster by Y to find natural row bands -> assign to row index in band order.
    2. Within each band, cluster by X to merge same-character pieces -> assign
       to column index in left-to-right order.
    Bands beyond ROWS or columns beyond COLUMNS are dropped."""
    row_gap = source_h / ROWS * row_gap_ratio
    column_gap = source_w / COLUMNS * column_gap_ratio
    bands = cluster_by_axis_gap(components, axis=1, gap_threshold=row_gap)

    cells: dict[tuple[int, int], list[dict]] = {}
    for row_index, band in enumerate(bands[:ROWS]):
        characters = cluster_by_axis_gap(band, axis=0, gap_threshold=column_gap)
        for column_index, char_components in enumerate(characters[:COLUMNS]):
            cells[(row_index, column_index)] = char_components
    return cells


def merge_bboxes(components: list[dict]) -> tuple[int, int, int, int]:
    left = min(c["bbox"][0] for c in components)
    top = min(c["bbox"][1] for c in components)
    right = max(c["bbox"][2] for c in components)
    bottom = max(c["bbox"][3] for c in components)
    return (left, top, right, bottom)


def decontaminate_edge_halo(
    rgba: Image.Image,
    edge_radius: int,
    content_threshold: int,
    bg_color: str,
) -> Image.Image:
    """Remove the colored halo on anti-aliased silhouette edges.

    A pixel is decontaminated when all of these hold:
    - it is opaque after edge background removal,
    - it lies within edge_radius pixels of any transparent pixel,
    - it is "tinted toward the background color" (see thresholds below).

    For bg_color='white' we reverse  observed = a * truecolor + (1 - a) * 255
    using a = (255 - min_ch) / 255 ; pixels whose min(R,G,B) >= content_threshold
    are treated as solid white content and skipped.

    For bg_color='black' we reverse  observed = a * truecolor + (1 - a) * 0
    using a = max_ch / 255 ; pixels whose max(R,G,B) <= content_threshold are
    treated as solid black content and skipped.

    The result is a pixel whose RGB is shifted away from the background color
    and whose alpha is reduced proportionally to how blended it was. Solid
    character pixels deeper than edge_radius from the silhouette are
    preserved."""
    arr = np.asarray(rgba, dtype=np.int32).copy()
    alpha = arr[..., 3]

    transparent = alpha == 0
    near = transparent.copy()
    for _ in range(edge_radius):
        d = np.zeros_like(near)
        d[1:, :]  |= near[:-1, :]
        d[:-1, :] |= near[1:, :]
        d[:, 1:]  |= near[:, :-1]
        d[:, :-1] |= near[:, 1:]
        near |= d

    rgb = arr[..., :3]
    if bg_color == "white":
        min_ch = rgb.min(axis=-1)
        boundary = (alpha > 0) & near & (min_ch < content_threshold)
        new_alpha = (255 - min_ch).clip(0, 255).astype(np.int32)
        new_alpha = np.minimum(new_alpha, alpha)
        safe = np.where(new_alpha > 0, new_alpha, 1)[..., None]
        decon = ((rgb - 255) * 255 // safe) + 255
    elif bg_color == "black":
        max_ch = rgb.max(axis=-1)
        boundary = (alpha > 0) & near & (max_ch > content_threshold)
        new_alpha = max_ch.clip(0, 255).astype(np.int32)
        new_alpha = np.minimum(new_alpha, alpha)
        safe = np.where(new_alpha > 0, new_alpha, 1)[..., None]
        decon = rgb * 255 // safe
    else:
        raise ValueError(f"bg_color must be 'white' or 'black', got {bg_color!r}")
    decon = decon.clip(0, 255)

    out = arr.copy()
    out[..., :3] = np.where(boundary[..., None], decon, rgb)
    out[..., 3] = np.where(boundary, new_alpha, alpha)
    return Image.fromarray(out.astype(np.uint8), "RGBA")


def premultiplied_resize(rgba: Image.Image, new_size: tuple[int, int]) -> Image.Image:
    """LANCZOS resize using premultiplied alpha. Without this, transparent
    pixels (which still hold white RGB) bleed into opaque neighbors and
    produce a visible white halo on dark characters."""
    arr = np.asarray(rgba, dtype=np.float32)
    alpha = arr[..., 3:4] / 255.0
    premul = np.empty_like(arr)
    premul[..., :3] = arr[..., :3] * alpha
    premul[..., 3] = arr[..., 3]
    premul_img = Image.fromarray(np.clip(premul, 0, 255).astype(np.uint8), "RGBA")
    resized = premul_img.resize(new_size, Image.Resampling.LANCZOS)

    arr2 = np.asarray(resized, dtype=np.float32)
    alpha2 = arr2[..., 3:4] / 255.0
    safe_alpha = np.where(alpha2 > 0, alpha2, 1.0)
    rgb2 = arr2[..., :3] / safe_alpha
    out = np.empty_like(arr2)
    out[..., :3] = rgb2
    out[..., 3] = arr2[..., 3]
    return Image.fromarray(np.clip(out, 0, 255).astype(np.uint8), "RGBA")


def fit_frame(rgba_crop: Image.Image, content_scale: float, bottom_padding_ratio: float) -> Image.Image:
    max_w = max(1, int(CELL_WIDTH * content_scale))
    max_h = max(1, int(CELL_HEIGHT * content_scale))
    scale = min(max_w / rgba_crop.width, max_h / rgba_crop.height, 1.0)
    new_w = max(1, round(rgba_crop.width * scale))
    new_h = max(1, round(rgba_crop.height * scale))
    if (new_w, new_h) != rgba_crop.size:
        rgba_crop = premultiplied_resize(rgba_crop, (new_w, new_h))

    cell = Image.new("RGBA", (CELL_WIDTH, CELL_HEIGHT), (0, 0, 0, 0))
    x = (CELL_WIDTH - new_w) // 2
    bottom_pad = max(0, int(CELL_HEIGHT * bottom_padding_ratio))
    y = CELL_HEIGHT - new_h - bottom_pad
    if y < 0:
        y = 0
    cell.paste(rgba_crop, (x, y), rgba_crop)
    return cell


def normalize_atlas(
    input_path: Path,
    output_dir: Path,
    pet_id: str,
    display_name: str,
    description: str,
    tolerance: int,
    content_scale: float,
    bottom_padding_ratio: float,
    alpha_threshold: int,
    min_component_area: int,
    row_gap_ratio: float,
    column_gap_ratio: float,
    pocket_max_area: int,
    edge_decon_radius: int,
    edge_decon_content_threshold: int,
    bg_color: str,
) -> None:
    source = Image.open(input_path).convert("RGB")
    rgba_full = remove_white_bg(
        source,
        tolerance=tolerance,
        pocket_max_area=pocket_max_area,
        bg_color=bg_color,
    )
    rgba_full = decontaminate_edge_halo(
        rgba_full,
        edge_radius=edge_decon_radius,
        content_threshold=edge_decon_content_threshold,
        bg_color=bg_color,
    )

    components = find_components(
        rgba_full,
        alpha_threshold=alpha_threshold,
        min_area=min_component_area,
    )
    cells = assign_to_cells(
        components,
        rgba_full.width,
        rgba_full.height,
        row_gap_ratio=row_gap_ratio,
        column_gap_ratio=column_gap_ratio,
    )

    atlas = Image.new("RGBA", (ATLAS_WIDTH, ATLAS_HEIGHT), (0, 0, 0, 0))
    for (row, column), comp_list in cells.items():
        bbox = merge_bboxes(comp_list)
        frame = rgba_full.crop(bbox)
        fitted = fit_frame(frame, content_scale, bottom_padding_ratio)
        atlas.paste(fitted, (column * CELL_WIDTH, row * CELL_HEIGHT), fitted)

    output_dir.mkdir(parents=True, exist_ok=True)
    atlas.save(output_dir / "spritesheet.png", "PNG", optimize=True)

    manifest = {
        "id": pet_id,
        "displayName": display_name,
        "description": description,
        "spritesheetPath": "spritesheet.png",
    }
    (output_dir / "pet.json").write_text(
        json.dumps(manifest, ensure_ascii=False, indent=2) + "\n",
        encoding="utf-8",
    )

    print(f"[ok] components: {len(components)}, cells filled: {len(cells)}")
    print(f"[ok] wrote {output_dir / 'spritesheet.png'}")
    print(f"[ok] wrote {output_dir / 'pet.json'}")


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Normalize a white-background pet sheet into a Codux 8x9 atlas."
    )
    parser.add_argument("input", help="Source pet sheet image (white background)")
    parser.add_argument("--output-dir", required=True)
    parser.add_argument("--id", required=True)
    parser.add_argument("--name", required=True)
    parser.add_argument("--description", default="Codux bundled pet atlas.")
    parser.add_argument("--tolerance", type=int, default=40,
                        help="Near-white tolerance for edge background removal (0-255). "
                             "Higher value catches more anti-aliased edge pixels as background.")
    parser.add_argument("--pocket-max-area", type=int, default=0,
                        help="When > 0, clear enclosed near-white islands up to this many "
                             "pixels. Off by default because the threshold cannot reliably "
                             "tell apart small body whites from leg/arm pockets; only enable "
                             "for sources where you have verified it does not erase content.")
    parser.add_argument("--content-scale", type=float, default=0.9,
                        help="Max frame content ratio inside each 192x208 cell (0-1).")
    parser.add_argument("--bottom-padding", type=float, default=0.06,
                        help="Bottom padding as fraction of cell height.")
    parser.add_argument("--alpha-threshold", type=int, default=8,
                        help="Alpha value above which a pixel is considered foreground.")
    parser.add_argument("--min-component-area", type=int, default=5000,
                        help="Drop connected components smaller than this many pixels (filters noise).")
    parser.add_argument("--row-gap-ratio", type=float, default=0.4,
                        help="A vertical gap larger than (source_h / 9) * ratio starts a new row band.")
    parser.add_argument("--column-gap-ratio", type=float, default=0.4,
                        help="A horizontal gap larger than (source_w / 8) * ratio starts a new character.")
    parser.add_argument("--edge-decon-radius", type=int, default=2,
                        help="Pixels within this distance of the silhouette boundary are "
                             "candidates for halo decontamination.")
    parser.add_argument("--edge-decon-content-threshold", type=int, default=240,
                        help="Skip decontamination for solid content pixels: with bg=white, "
                             "skip pixels whose min(R,G,B) >= threshold; with bg=black, skip "
                             "pixels whose max(R,G,B) <= (255 - threshold).")
    parser.add_argument("--bg-color", choices=["white", "black"], default="white",
                        help="Source background color. Use 'black' for white/light-colored "
                             "characters drawn on a black sheet so wool/fur is never confused "
                             "with the background.")
    args = parser.parse_args()

    normalize_atlas(
        input_path=Path(args.input).expanduser().resolve(),
        output_dir=Path(args.output_dir).expanduser().resolve(),
        pet_id=args.id,
        display_name=args.name,
        description=args.description,
        tolerance=max(0, min(args.tolerance, 255)),
        content_scale=max(0.1, min(args.content_scale, 1.0)),
        bottom_padding_ratio=max(0.0, min(args.bottom_padding, 0.4)),
        alpha_threshold=max(0, min(args.alpha_threshold, 255)),
        min_component_area=max(1, args.min_component_area),
        row_gap_ratio=max(0.05, min(args.row_gap_ratio, 0.95)),
        column_gap_ratio=max(0.05, min(args.column_gap_ratio, 0.95)),
        pocket_max_area=max(0, args.pocket_max_area),
        edge_decon_radius=max(0, args.edge_decon_radius),
        edge_decon_content_threshold=(
            max(0, min(args.edge_decon_content_threshold, 255))
            if args.bg_color == "white"
            else max(0, min(255 - args.edge_decon_content_threshold, 255))
        ),
        bg_color=args.bg_color,
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
