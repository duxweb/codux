"""白底去背 + 调色盘量化公共模块。
1. BFS 泛洪填充去除连通白色背景。
2. 可选清理被图形包裹的内部大块白岛。
3. 自动从帧集合中提取 N 色调色盘(所有帧共用,保证动画颜色一致)。
4. 量化时不做 dither,产生硬边像素风效果,同时消除白边问题。"""

from __future__ import annotations
from collections import deque
from PIL import Image


# ── BFS 去背 ─────────────────────────────────────────────────────────────────

def remove_white_bg(
    img: Image.Image,
    tolerance: int = 30,
    internal_island_min_area: int = 0,
    internal_island_rules: list[dict] | None = None,
    pocket_max_area: int = 0,
    bg_color: str = "white",
) -> Image.Image:
    """从四边 BFS,去除与边界连通的纯色背景（白底或黑底）。

    bg_color="white" 时，把 R/G/B 都 >= 255-tolerance 的像素视作背景；
    bg_color="black" 时，把 R/G/B 都 <= tolerance 的像素视作背景。
    用于不同生成素材的纯色底（推荐白色物种用黑底素材）。

    internal_island_min_area > 0 时，还会额外删除不接触边界的同色大连通域。
    pocket_max_area > 0 时，删除被角色完全包裹、面积 <= 阈值的同色小连通域
    （腿间、腋下等被围合的背景口袋）；大块同色身体内容因面积超阈值得以保留。
    """
    rgb = img.convert("RGB")
    w, h = rgb.size
    px = rgb.load()
    assert px is not None

    if bg_color == "white":
        def is_bg(x: int, y: int) -> bool:
            r, g, b = px[x, y]
            return r >= 255 - tolerance and g >= 255 - tolerance and b >= 255 - tolerance
    elif bg_color == "black":
        def is_bg(x: int, y: int) -> bool:
            r, g, b = px[x, y]
            return r <= tolerance and g <= tolerance and b <= tolerance
    else:
        raise ValueError(f"bg_color must be 'white' or 'black', got {bg_color!r}")

    visited = [[False] * h for _ in range(w)]
    bg_mask = [[False] * h for _ in range(w)]
    queue: deque[tuple[int, int]] = deque()

    for x in range(w):
        for y in (0, h - 1):
            if is_bg(x, y) and not visited[x][y]:
                visited[x][y] = True
                queue.append((x, y))
    for y in range(h):
        for x in (0, w - 1):
            if is_bg(x, y) and not visited[x][y]:
                visited[x][y] = True
                queue.append((x, y))

    while queue:
        cx, cy = queue.popleft()
        bg_mask[cx][cy] = True
        for dx, dy in ((-1, 0), (1, 0), (0, -1), (0, 1)):
            nx, ny = cx + dx, cy + dy
            if 0 <= nx < w and 0 <= ny < h and not visited[nx][ny] and is_bg(nx, ny):
                visited[nx][ny] = True
                queue.append((nx, ny))

    if internal_island_min_area > 0 or internal_island_rules or pocket_max_area > 0:
        for x in range(w):
            for y in range(h):
                if visited[x][y] or not is_bg(x, y):
                    continue
                component: list[tuple[int, int]] = []
                touches_edge = False
                minx = maxx = x
                miny = maxy = y
                queue.append((x, y))
                visited[x][y] = True
                while queue:
                    cx, cy = queue.popleft()
                    component.append((cx, cy))
                    minx = min(minx, cx)
                    maxx = max(maxx, cx)
                    miny = min(miny, cy)
                    maxy = max(maxy, cy)
                    if cx == 0 or cy == 0 or cx == w - 1 or cy == h - 1:
                        touches_edge = True
                    for dx, dy in ((-1, 0), (1, 0), (0, -1), (0, 1)):
                        nx, ny = cx + dx, cy + dy
                        if 0 <= nx < w and 0 <= ny < h and not visited[nx][ny] and is_bg(nx, ny):
                            visited[nx][ny] = True
                            queue.append((nx, ny))

                should_clear = False
                if not touches_edge and internal_island_min_area > 0 and len(component) >= internal_island_min_area:
                    should_clear = True

                if not touches_edge and pocket_max_area > 0 and len(component) <= pocket_max_area:
                    should_clear = True

                if not touches_edge and internal_island_rules:
                    cx = (minx + maxx) // 2
                    cy = (miny + maxy) // 2
                    for rule in internal_island_rules:
                        min_area = int(rule.get("min_area", 0))
                        max_area = int(rule.get("max_area", 1 << 60))
                        rect = rule.get("rect")
                        if len(component) < min_area or len(component) > max_area or not rect:
                            continue
                        x1, y1, x2, y2 = [int(v) for v in rect]
                        if x1 <= cx <= x2 and y1 <= cy <= y2:
                            should_clear = True
                            break

                if should_clear:
                    for cx, cy in component:
                        bg_mask[cx][cy] = True

    rgba = img.convert("RGBA")
    apx = rgba.load()
    assert apx is not None
    for x in range(w):
        for y in range(h):
            if bg_mask[x][y]:
                r, g, b, _ = apx[x, y]
                apx[x, y] = (r, g, b, 0)
    return rgba


# ── 多帧一致量化 ──────────────────────────────────────────────────────────────

def build_shared_palette(rgbas: list[Image.Image],
                         num_colors: int = 64) -> Image.Image:
    """把所有帧拼成一张大图,提取共用调色盘,返回量化后的参考图(用于 apply_palette)。"""
    if not rgbas:
        raise ValueError("空帧列表")
    w, h = rgbas[0].size
    strip = Image.new("RGB", (w, h * len(rgbas)))
    for i, fr in enumerate(rgbas):
        strip.paste(fr.convert("RGB"), (0, i * h))
    return strip.quantize(colors=num_colors,
                          method=Image.Quantize.MEDIANCUT, dither=0)


def apply_palette(rgba: Image.Image, palette_ref: Image.Image) -> Image.Image:
    """将 RGBA 图的 RGB 部分量化到 palette_ref 的调色盘,保留原 alpha。"""
    alpha = rgba.split()[3]
    rgb_q = rgba.convert("RGB").quantize(palette=palette_ref, dither=0)
    rgb   = rgb_q.convert("RGB")
    r, g, b = rgb.split()
    return Image.merge("RGBA", (r, g, b, alpha))
