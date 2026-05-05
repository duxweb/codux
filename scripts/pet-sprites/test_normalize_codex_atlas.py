"""Unit tests for normalize_codex_atlas. Run with `python3 -m unittest`.

These tests use synthetic in-memory PIL images so they do not depend on
any source asset on disk."""

from __future__ import annotations

import sys
import unittest
from pathlib import Path

from PIL import Image, ImageDraw

sys.path.insert(0, str(Path(__file__).resolve().parent))
from normalize_codex_atlas import (
    ATLAS_HEIGHT,
    ATLAS_WIDTH,
    CELL_HEIGHT,
    CELL_WIDTH,
    COLUMNS,
    ROWS,
    assign_to_cells,
    cluster_by_axis_gap,
    compute_atlas_scale,
    decontaminate_edge_halo,
    find_components,
    merge_bboxes,
    normalize_atlas,
    place_frame,
    premultiplied_resize,
)


def _white(width: int, height: int) -> Image.Image:
    return Image.new("RGB", (width, height), (255, 255, 255))


def _to_rgba_with_alpha_for_non_white(rgb: Image.Image) -> Image.Image:
    """Helper that mimics what remove_white_bg outputs: white pixels become
    transparent, everything else stays opaque."""
    rgba = rgb.convert("RGBA")
    pixels = rgba.load()
    for y in range(rgba.height):
        for x in range(rgba.width):
            r, g, b, _ = pixels[x, y]
            if r >= 250 and g >= 250 and b >= 250:
                pixels[x, y] = (r, g, b, 0)
    return rgba


class FindComponentsTests(unittest.TestCase):
    def test_finds_two_disjoint_blobs(self) -> None:
        rgb = _white(40, 40)
        draw = ImageDraw.Draw(rgb)
        draw.rectangle((2, 2, 11, 11), fill=(10, 20, 30))
        draw.rectangle((25, 25, 34, 34), fill=(40, 50, 60))
        rgba = _to_rgba_with_alpha_for_non_white(rgb)

        components = find_components(rgba, alpha_threshold=8, min_area=10)

        self.assertEqual(len(components), 2)
        components.sort(key=lambda c: c["centroid"][0])
        self.assertEqual(components[0]["bbox"], (2, 2, 12, 12))
        self.assertEqual(components[1]["bbox"], (25, 25, 35, 35))
        self.assertAlmostEqual(components[0]["centroid"][0], 6.5, delta=0.5)
        self.assertAlmostEqual(components[0]["centroid"][1], 6.5, delta=0.5)

    def test_drops_components_below_min_area(self) -> None:
        rgb = _white(20, 20)
        draw = ImageDraw.Draw(rgb)
        draw.rectangle((2, 2, 3, 3), fill=(0, 0, 0))   # 4 px
        draw.rectangle((10, 10, 14, 14), fill=(0, 0, 0))  # 25 px
        rgba = _to_rgba_with_alpha_for_non_white(rgb)

        components = find_components(rgba, alpha_threshold=8, min_area=10)

        self.assertEqual(len(components), 1)
        self.assertEqual(components[0]["bbox"], (10, 10, 15, 15))

    def test_keeps_internal_white_when_alpha_already_transparent_outside(self) -> None:
        # Square shell with a white interior -- the white interior pixels here
        # are *opaque* white because the input came from somewhere that did
        # NOT mark them as transparent. find_components must include them.
        rgba = Image.new("RGBA", (20, 20), (255, 255, 255, 0))   # transparent bg
        draw = ImageDraw.Draw(rgba)
        # Solid filled square fully opaque, including interior white pixels.
        for y in range(4, 16):
            for x in range(4, 16):
                rgba.putpixel((x, y), (255, 255, 255, 255))
        # outline a darker border
        draw.rectangle((4, 4, 15, 15), outline=(20, 20, 20))

        components = find_components(rgba, alpha_threshold=8, min_area=10)

        self.assertEqual(len(components), 1)
        self.assertEqual(components[0]["area"], 144)
        self.assertEqual(components[0]["bbox"], (4, 4, 16, 16))


class ClusterByAxisGapTests(unittest.TestCase):
    def test_groups_when_gap_smaller_than_threshold(self) -> None:
        comps = [{"centroid": (10, 0)}, {"centroid": (15, 0)}, {"centroid": (50, 0)}]
        clusters = cluster_by_axis_gap(comps, axis=0, gap_threshold=10)
        self.assertEqual(len(clusters), 2)
        self.assertEqual(len(clusters[0]), 2)
        self.assertEqual(len(clusters[1]), 1)


class AssignToCellsTests(unittest.TestCase):
    def test_two_y_bands_two_x_clusters(self) -> None:
        # source 800x900 -> cell ~100x100; gap_ratio 0.4 -> y_gap ~40, x_gap ~40
        comps = [
            {"centroid": (50, 50), "bbox": (0, 0, 1, 1), "area": 1},
            {"centroid": (60, 52), "bbox": (0, 0, 1, 1), "area": 1},   # same band, same character
            {"centroid": (250, 50), "bbox": (0, 0, 1, 1), "area": 1},  # same band, next character
            {"centroid": (50, 850), "bbox": (0, 0, 1, 1), "area": 1},  # second band
        ]
        cells = assign_to_cells(comps, 800, 900, row_gap_ratio=0.4, column_gap_ratio=0.4)
        # First band -> row 0; two characters -> col 0 and col 1
        self.assertIn((0, 0), cells)
        self.assertIn((0, 1), cells)
        self.assertEqual(len(cells[(0, 0)]), 2)  # body+highlight merged
        # Second band -> row 1
        self.assertIn((1, 0), cells)

    def test_truncates_extra_rows_and_columns(self) -> None:
        # 10 separated Y bands -> only ROWS=9 should be kept.
        comps = [{"centroid": (10, 50 + i * 200), "bbox": (0, 0, 1, 1), "area": 1}
                 for i in range(ROWS + 1)]
        cells = assign_to_cells(comps, 800, 2000, row_gap_ratio=0.4, column_gap_ratio=0.4)
        self.assertEqual(max(r for (r, _) in cells), ROWS - 1)


class MergeBBoxesTests(unittest.TestCase):
    def test_unions_disjoint_boxes(self) -> None:
        comps = [
            {"bbox": (10, 20, 30, 40)},
            {"bbox": (5, 25, 25, 50)},
            {"bbox": (15, 18, 35, 42)},
        ]
        self.assertEqual(merge_bboxes(comps), (5, 18, 35, 50))


class DecontaminateEdgeHaloTests(unittest.TestCase):
    def test_white_bg_strips_gray_halo_on_dark_silhouette(self) -> None:
        """White-bg case: a dark blob with a gray AA ring -- the ring must
        shift toward true dark + drop alpha; the dark core stays intact;
        solid white content nearby must NOT be decontaminated."""
        rgba = Image.new("RGBA", (10, 10), (0, 0, 0, 0))
        for y in range(10):
            rgba.putpixel((2, y), (128, 128, 128, 255))   # halo column
            for x in range(3, 9):
                rgba.putpixel((x, y), (0, 0, 0, 255))     # solid dark
            rgba.putpixel((9, y), (255, 255, 255, 255))   # white content

        result = decontaminate_edge_halo(rgba, edge_radius=2,
                                         content_threshold=240, bg_color="white")

        for y in range(10):
            r, g, b, a = result.getpixel((2, y))
            self.assertLess(a, 200, f"halo alpha at y={y} should drop")
            self.assertLess(max(r, g, b), 60, f"halo RGB at y={y} near-black")
        self.assertEqual(result.getpixel((5, 5)), (0, 0, 0, 255))
        self.assertEqual(result.getpixel((9, 5)), (255, 255, 255, 255))

    def test_black_bg_strips_gray_halo_on_white_silhouette(self) -> None:
        """Black-bg case (mirror image): a white blob with a gray AA ring
        on a transparent (originally black) bg. The gray ring must shift
        toward white + drop alpha; the white core stays opaque white;
        solid black content nearby must NOT be decontaminated."""
        rgba = Image.new("RGBA", (10, 10), (0, 0, 0, 0))
        for y in range(10):
            rgba.putpixel((2, y), (128, 128, 128, 255))    # halo column
            for x in range(3, 9):
                rgba.putpixel((x, y), (255, 255, 255, 255))  # solid white wool
            rgba.putpixel((9, y), (0, 0, 0, 255))           # solid black content

        # threshold 15 -> on black bg: skip pixels with max(R,G,B) <= 15.
        result = decontaminate_edge_halo(rgba, edge_radius=2,
                                         content_threshold=15, bg_color="black")

        for y in range(10):
            r, g, b, a = result.getpixel((2, y))
            self.assertLess(a, 200, f"halo alpha at y={y} should drop")
            self.assertGreater(min(r, g, b), 200,
                               f"halo RGB at y={y} should shift toward white")
        # Core white wool stays solid white opaque.
        self.assertEqual(result.getpixel((5, 5)), (255, 255, 255, 255))
        # Solid black content pixel preserved (max(R,G,B)=0 <= 15 -> skipped).
        self.assertEqual(result.getpixel((9, 5)), (0, 0, 0, 255))


class PremultipliedResizeTests(unittest.TestCase):
    def test_no_white_halo_around_dark_blob(self) -> None:
        """A black opaque circle on a fully-transparent (RGB=255 white) bg
        must not develop a bright halo after downscaling. The opaque center
        must stay roughly black; semi-transparent edge pixels must not be
        biased toward white."""
        size = 80
        rgba = Image.new("RGBA", (size, size), (255, 255, 255, 0))
        draw = ImageDraw.Draw(rgba)
        draw.ellipse((20, 20, 59, 59), fill=(0, 0, 0, 255))

        small = premultiplied_resize(rgba, (16, 16))
        self.assertEqual(small.size, (16, 16))

        center = small.getpixel((8, 8))
        # Center pixel should be fully opaque dark, NOT polluted by white.
        self.assertEqual(center[3], 255)
        self.assertLess(max(center[:3]), 30)

        # Any partially-transparent edge pixel must have RGB close to black,
        # not biased white -- that would mean the halo crept back.
        for x in range(16):
            for y in range(16):
                r, g, b, a = small.getpixel((x, y))
                if 0 < a < 255:
                    self.assertLess(
                        max(r, g, b), 80,
                        f"edge pixel at ({x},{y}) has bright RGB ({r},{g},{b}) "
                        "-> halo regression",
                    )


class ComputeAtlasScaleTests(unittest.TestCase):
    def test_picks_scale_to_fit_largest_frame(self) -> None:
        # Largest frame is 200x180. content_scale 0.9 -> target 172.8x187.2.
        # scale = min(172.8/200, 187.2/180, 1.0) = 0.864 (width-limited).
        sizes = [(150, 180), (200, 180), (170, 175)]
        s = compute_atlas_scale(sizes, content_scale=0.9)
        self.assertAlmostEqual(s, 172.8 / 200, delta=0.001)

    def test_caps_scale_at_one(self) -> None:
        # Tiny frames must NOT be upscaled (sprites would blur).
        sizes = [(20, 20), (30, 25)]
        self.assertEqual(compute_atlas_scale(sizes, content_scale=0.9), 1.0)


class PlaceFrameTests(unittest.TestCase):
    def test_uniform_scale_preserves_relative_size(self) -> None:
        """Two frames of different widths placed with the SAME scale must keep
        their width ratio in the output (no per-frame fit-to-cell)."""
        wide = Image.new("RGBA", (200, 180), (200, 30, 30, 255))
        narrow = Image.new("RGBA", (140, 180), (200, 30, 30, 255))
        scale = compute_atlas_scale([wide.size, narrow.size], content_scale=0.9)

        wide_cell = place_frame(wide, scale, bottom_padding_ratio=0.06)
        narrow_cell = place_frame(narrow, scale, bottom_padding_ratio=0.06)

        wide_bbox = wide_cell.getbbox()
        narrow_bbox = narrow_cell.getbbox()
        wide_w = wide_bbox[2] - wide_bbox[0]
        narrow_w = narrow_bbox[2] - narrow_bbox[0]
        wide_h = wide_bbox[3] - wide_bbox[1]
        narrow_h = narrow_bbox[3] - narrow_bbox[1]
        # Heights identical at same scale.
        self.assertEqual(wide_h, narrow_h)
        # Width ratio in output matches the ratio in source (140/200 = 0.7).
        self.assertAlmostEqual(narrow_w / wide_w, 140 / 200, delta=0.02)
        # Both anchored to the same bottom row.
        self.assertEqual(wide_bbox[3], narrow_bbox[3])

    def test_output_is_cell_sized_and_bottom_anchored(self) -> None:
        crop = Image.new("RGBA", (60, 40), (200, 30, 30, 255))
        cell = place_frame(crop, scale=1.0, bottom_padding_ratio=0.06)
        self.assertEqual(cell.size, (CELL_WIDTH, CELL_HEIGHT))
        left, top, right, bottom = cell.getbbox()
        self.assertEqual(bottom, CELL_HEIGHT - int(CELL_HEIGHT * 0.06))
        self.assertAlmostEqual((left + right) / 2, CELL_WIDTH / 2, delta=2)


class EndToEndAtlasTests(unittest.TestCase):
    def test_synthetic_white_grid_produces_clean_atlas(self) -> None:
        """Build an 800x900 pseudo-source with a fixed per-row frame count
        (matches Codux atlas spec [6,8,8,4,5,8,6,6,6]). Frames in each row are
        packed left-to-right, leaving the rightmost cells empty -- this is the
        layout real AI-generated sheets use. After normalization, the atlas
        should be 1536x1872 RGBA, every expected (row, col) should contain an
        opaque pixel, and every row-tail empty cell should stay fully
        transparent."""
        source_w = 800
        source_h = 900
        rgb = _white(source_w, source_h)
        draw = ImageDraw.Draw(rgb)
        row_counts = [6, 8, 8, 4, 5, 8, 6, 6, 6]

        cell_w = source_w / COLUMNS
        cell_h = source_h / ROWS
        for r in range(ROWS):
            for c in range(row_counts[r]):
                cx = (c + 0.5) * cell_w
                cy = (r + 0.5) * cell_h
                color = ((r * 25) % 256, (c * 31) % 256, 90)
                draw.rectangle(
                    (int(cx - 15), int(cy - 15), int(cx + 15), int(cy + 15)),
                    fill=color,
                )
        source_path = self._tmp("source.png")
        rgb.save(source_path, "PNG")

        out_dir = self._tmp_dir("out")
        normalize_atlas(
            input_path=source_path,
            output_dir=out_dir,
            pet_id="test-default",
            display_name="test",
            description="unit test",
            tolerance=18,
            content_scale=0.9,
            bottom_padding_ratio=0.06,
            alpha_threshold=8,
            min_component_area=50,
            row_gap_ratio=0.4,
            column_gap_ratio=0.4,
            pocket_max_area=0,
            edge_decon_radius=2,
            edge_decon_content_threshold=240,
            bg_color="white",
        )

        atlas = Image.open(out_dir / "spritesheet.png")
        self.assertEqual(atlas.size, (ATLAS_WIDTH, ATLAS_HEIGHT))
        self.assertEqual(atlas.mode, "RGBA")

        alpha = atlas.split()[3]
        for r in range(ROWS):
            for c in range(COLUMNS):
                box = (c * CELL_WIDTH, r * CELL_HEIGHT,
                       (c + 1) * CELL_WIDTH, (r + 1) * CELL_HEIGHT)
                cell_alpha = alpha.crop(box)
                lo, hi = cell_alpha.getextrema()
                if c < row_counts[r]:
                    self.assertGreater(hi, 0, f"cell ({r},{c}) should be filled")
                else:
                    self.assertEqual(
                        (lo, hi), (0, 0),
                        f"cell ({r},{c}) should stay transparent",
                    )

    # --- tmpfile helpers ------------------------------------------------

    def setUp(self) -> None:
        import tempfile
        self._tmp_root = Path(tempfile.mkdtemp(prefix="atlas-test-"))

    def tearDown(self) -> None:
        import shutil
        shutil.rmtree(self._tmp_root, ignore_errors=True)

    def _tmp(self, name: str) -> Path:
        return self._tmp_root / name

    def _tmp_dir(self, name: str) -> Path:
        path = self._tmp_root / name
        path.mkdir(parents=True, exist_ok=True)
        return path


if __name__ == "__main__":
    unittest.main()
