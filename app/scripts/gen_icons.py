"""Generate EnvHive app icons (pure stdlib, no Pillow).

Hive motif: four flat-top hexagons in Microsoft's four brand colors
(red / green / blue / yellow), tightly honeycombed — top, center,
bottom-left, bottom-right share edges with no gaps.
Each hexagon keeps a thin dark honeycomb border so the hive texture
stays readable at tiny sizes.

Anti-aliasing strategy: render one 512px master image, then produce
smaller sizes (256 / 128 / 32px) via box-filter downsampling with
premultiplied alpha.
Outputs into src-tauri/icons/.
"""
import math
import os
import struct
import zlib

# 微软四色（官方品牌色）与深棕蜂窝壁
MS_RED = (242, 80, 34)        # 红
MS_GREEN = (127, 186, 0)      # 绿
MS_BLUE = (0, 164, 239)       # 蓝
MS_YELLOW = (255, 185, 0)     # 黄
BORDER = (94, 66, 30)         # 深棕（蜂窝壁）
# 顺序对应：上、中、左下、右下
HIVE4_COLORS = [MS_RED, MS_GREEN, MS_BLUE, MS_YELLOW]


def png_chunk(tag: bytes, data: bytes) -> bytes:
    return (
        struct.pack(">I", len(data))
        + tag
        + data
        + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
    )


def hex_vertices(cx: float, cy: float, r: float):
    """平顶六边形顶点（顶点在左右），角度从 0° 起每 60° 一个。"""
    return [
        (cx + r * math.cos(math.radians(60 * i)),
         cy + r * math.sin(math.radians(60 * i)))
        for i in range(6)
    ]


def in_polygon(x: float, y: float, verts) -> bool:
    """凸多边形内判定（符号测试）。"""
    neg = pos = False
    n = len(verts)
    for i in range(n):
        ax, ay = verts[i]
        bx, by = verts[(i + 1) % n]
        cross = (bx - ax) * (y - ay) - (by - ay) * (x - ax)
        if cross < 0:
            neg = True
        elif cross > 0:
            pos = True
        if neg and pos:
            return False
    return True


def hive4_centers(cx: float, cy: float, r: float):
    """上、中、左下、右下四格密排（平顶六边形蜂窝，相互共享边、无缝隙）。

    邻居偏移（屏幕坐标，y 向下）：
      上   = (0, -sqrt(3) * r)
      左下 = (-1.5 * r, +0.866 * r)
      右下 = (+1.5 * r, +0.866 * r)
    """
    h = r * math.sqrt(3)
    return [
        (cx, cy - h),                       # 上
        (cx, cy),                           # 中
        (cx - 1.5 * r, cy + 0.866 * r),     # 左下
        (cx + 1.5 * r, cy + 0.866 * r),     # 右下
    ]


def dist_to_segment(px: float, py: float, a, b) -> float:
    """点到线段的最短距离。"""
    ax, ay = a
    bx, by = b
    dx, dy = bx - ax, by - ay
    if dx == 0 and dy == 0:
        return math.hypot(px - ax, py - ay)
    t = ((px - ax) * dx + (py - ay) * dy) / (dx * dx + dy * dy)
    t = 0.0 if t < 0.0 else (1.0 if t > 1.0 else t)
    return math.hypot(px - (ax + t * dx), py - (ay + t * dy))


def build_pixels(size: int):
    """渲染 512px 主图（微软四色密排四格六边形，细深棕边框）。

    边界按「像素到最近六边形边的距离」判定：距离小于半宽即涂深棕，
    与 icon.svg 中 stroke 居中绘制一致——
    共享边两侧各贡献半宽合成一条标准宽度边（接触边公用宽度），
    外边缘向内半宽 + 向外（stroke 外溢）半宽，宽度全局统一不叠加。
    """
    scale = size / 512.0
    r = 90 * scale
    cx, cy = size * 0.5, size * 0.5
    centers = hive4_centers(cx, cy, r)
    colors = HIVE4_COLORS
    # 边界半宽（法向）：与旧 inner=0.92r 环带等效，即 (r - 0.92r) * (sqrt(3)/2) / 2
    border_half_w = r * 0.08 * math.sqrt(3) / 4
    # 预生成所有顶点与边（避免逐像素重复计算三角函数）
    outer = [hex_vertices(hx, hy, r) for hx, hy in centers]
    edges = [[(v[j], v[(j + 1) % 6]) for j in range(6)] for v in outer]
    pixels = []
    for y in range(size):
        row = []
        for x in range(size):
            px, py = x + 0.5, y + 0.5
            color = None
            for i in range(4):
                if in_polygon(px, py, outer[i]):
                    dmin = min(dist_to_segment(px, py, a, b) for a, b in edges[i])
                    color = BORDER if dmin < border_half_w else colors[i]
                    break
            if color is None:
                # 六边形外部的 stroke 外溢半宽（模拟 SVG stroke 居中的外凸部分）
                for i in range(4):
                    dmin = min(dist_to_segment(px, py, a, b) for a, b in edges[i])
                    if dmin < border_half_w:
                        color = BORDER
                        break
            row.append((*color, 255) if color else (0, 0, 0, 0))
        pixels.append(row)
    return pixels


def pack(pixels, size: int) -> bytes:
    """Flatten RGBA pixels into PNG scanlines (filter byte 0 per row)."""
    rows = bytearray()
    for row in pixels:
        rows.append(0)
        for r, g, b, a in row:
            rows += bytes([r, g, b, a])
    return bytes(rows)


def downsample(pixels, size: int, factor: int) -> bytes:
    """Box-filter downscale with premultiplied alpha (anti-aliased)."""
    rows = bytearray()
    for y in range(size):
        rows.append(0)
        for x in range(size):
            pr = pg = pb = pa = 0
            for sy in range(factor):
                for sx in range(factor):
                    r, g, b, a = pixels[y * factor + sy][x * factor + sx]
                    pr += r * a
                    pg += g * a
                    pb += b * a
                    pa += a
            if pa:
                rows += bytes(
                    [pr // pa, pg // pa, pb // pa, pa // (factor * factor)]
                )
            else:
                rows += bytes([0, 0, 0, 0])
    return bytes(rows)


def encode_png(rows: bytes, size: int) -> bytes:
    """封装 RGBA scanlines 为完整 PNG 字节流。"""
    png = b"\x89PNG\r\n\x1a\n"
    png += png_chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0))
    png += png_chunk(b"IDAT", zlib.compress(rows))
    png += png_chunk(b"IEND", b"")
    return png


def make_png(rows: bytes, size: int, path: str) -> None:
    with open(path, "wb") as f:
        f.write(encode_png(rows, size))


def make_ico(rows: bytes, size: int, path: str) -> None:
    """ICO 内嵌图像必须是完整 PNG 字节流（Windows 解析要求），
    不能直接写入裸 scanlines。"""
    png = encode_png(rows, size)
    header = struct.pack("<HHH", 0, 1, 1)
    entry = struct.pack("<BBBBHHII", 0, 0, 0, 0, 1, 32, len(png), 22)
    with open(path, "wb") as f:
        f.write(header + entry + png)


def main() -> None:
    out = os.path.join(os.path.dirname(__file__), "..", "src-tauri", "icons")
    os.makedirs(out, exist_ok=True)
    full = build_pixels(512)
    # 512 主图超采样 → 各尺寸 box 降采样（抗锯齿）
    png256 = downsample(full, 256, 2)
    make_png(png256, 256, os.path.join(out, "icon.png"))
    make_png(png256, 256, os.path.join(out, "128x128@2x.png"))
    make_png(downsample(full, 128, 4), 128, os.path.join(out, "128x128.png"))
    make_png(downsample(full, 32, 16), 32, os.path.join(out, "32x32.png"))
    make_ico(png256, 256, os.path.join(out, "icon.ico"))
    print("icons generated ->", out)


if __name__ == "__main__":
    main()
