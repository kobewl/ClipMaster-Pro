#!/usr/bin/env python3
"""生成 macOS 菜单栏托盘图标（模板图）。

设计来源：docs/design/logo/clipmaster-logo-mono.svg 的几何（剪贴板剪影），
但**不是**直接栅格化那个 SVG —— 那个文件的单色版是一整块实心圆角矩形，
缩到菜单栏的 18pt 后就是一块没有辨识度的砖（实测截图：白色方块）。

这里改画「描边板身 + 实心夹子」的经典剪贴板轮廓：只保留最外轮廓，
不画内容线 —— 内部细节在 18pt 下会并拢成灰团（设计文档「≤24px 切换剪影版」
说的就是这个）。描边线宽按 44 坐标系给 3px，缩到 18pt 后约 1.2pt，
在 Retina 上仍然锐利。

输出规格（macOS 模板图硬性要求）：
- 44×44 像素（= 22pt @2x，系统会自行缩放到 18pt 高）
- 纯黑 RGB(0,0,0) + 透明背景，alpha 通道即形状遮罩
- 由系统按菜单栏明暗自动反色（icon_as_template(true)）

用法：
    python3 scripts/gen_tray_icon.py
"""
from pathlib import Path

from PIL import Image, ImageDraw

# 最终像素尺寸。44 是 22pt 的 @2x 资源，tray-icon 会再缩到 18pt 高显示。
SIZE = 44
# 超采样倍数：先在 4 倍画布上画，再降采样，边缘才不会有锯齿。
SUPERSAMPLE = 4
CANVAS = SIZE * SUPERSAMPLE

# 44 坐标系里的几何参数（改动这里就等于改图标外观）。
PLATE_LEFT, PLATE_TOP = 8.5, 11.5
PLATE_RIGHT, PLATE_BOTTOM = 35.5, 39.0
PLATE_RADIUS = 5.5
PLATE_STROKE = 3.0

CLIP_LEFT, CLIP_TOP = 16.0, 4.5
CLIP_RIGHT, CLIP_BOTTOM = 28.0, 14.5
CLIP_RADIUS = 4.5


def _px(value: float) -> float:
    """把 44 坐标系的数值换算到超采样画布。"""
    return value * SUPERSAMPLE


def build_mask() -> Image.Image:
    """画出形状遮罩（L 通道，255 = 实心）。"""
    mask = Image.new("L", (CANVAS, CANVAS), 0)
    draw = ImageDraw.Draw(mask)

    # 板身：只描边，中间镂空 —— 轮廓在 18pt 下比实心块更容易辨认。
    draw.rounded_rectangle(
        [_px(PLATE_LEFT), _px(PLATE_TOP), _px(PLATE_RIGHT), _px(PLATE_BOTTOM)],
        radius=_px(PLATE_RADIUS),
        outline=255,
        width=round(_px(PLATE_STROKE)),
    )

    # 夹子：实心，压在板顶边缘上（标准剪贴板画法）。
    draw.rounded_rectangle(
        [_px(CLIP_LEFT), _px(CLIP_TOP), _px(CLIP_RIGHT), _px(CLIP_BOTTOM)],
        radius=_px(CLIP_RADIUS),
        fill=255,
    )

    return mask.resize((SIZE, SIZE), Image.LANCZOS)


def build_icon() -> Image.Image:
    """遮罩 → 纯黑 + 透明背景的 RGBA 图标。"""
    mask = build_mask()
    black = Image.new("L", (SIZE, SIZE), 0)
    return Image.merge("RGBA", (black, black, black, mask))


def main() -> None:
    out = Path(__file__).resolve().parent.parent / "src-tauri" / "icons" / "tray-icon.png"
    build_icon().save(out)
    print(f"已生成 {out}  ({SIZE}×{SIZE}, 纯黑模板图)")


if __name__ == "__main__":
    main()
