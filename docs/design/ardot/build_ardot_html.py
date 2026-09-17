#!/usr/bin/env python3
"""
把 prototype.html 的 13 屏画板拆成 13 个「自包含单屏 HTML」。

用途：交给 Ardot MCP 的 html_to_ardot 逐屏导入，导入后每屏是一个独立的
可编辑画板（Frame），而不是一整页长图。

做法（关键：不抽取、只复用）
  prototype.html 里的定义（menuOverlay / settingsDialog / emptyScreen …）是**夹在**
  board.push 之间写的，按关键字切割会切坏依赖。所以这里让整段原型脚本原样跑完，
  只在末尾从内存里的 board 数组重新取屏 —— 渲染结果与原型 100% 一致。

  1. 整段 <script> 原样保留，并给它一个隐藏的 <div id="board"> 当落点，
     让原脚本能正常执行到底（board 数组被填满）。
  2. 追加一段取屏逻辑：把 board 前 6 项（5 个 .pair + 1 个 .grid3）按顺序摊平，
     去掉每格顶部的说明文字 <p class="caption">，得到 13 个屏。
  3. 把 proto.css 内联进 <style>（Ardot 从远端拉取 HTML，外链 CSS 会 404）。
  4. 追加覆盖样式：去掉窗口圆角/投影/标题栏，让每屏是干净的 800×600 矩形，
     并让列表区吃满剩余高度（否则状态栏会被挤出 600px 视口）。

用法：
    python3 build_ardot_html.py [输出目录]     # 默认 /tmp/ardot_html
"""

import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
PROTO_DIR = os.path.join(os.path.dirname(HERE), "prototype")

# 屏名 → 在 board 摊平后的下标（顺序由 prototype.html 的 board.push 顺序决定）
SCREENS = [
    "01-主界面-浅色",
    "02-主界面-深色",
    "03-搜索命中",
    "04-行悬停",
    "05-分组菜单",
    "06-图片记录",
    "07-偏好设置",
    "08-管理分组",
    "09-危险操作确认",
    "10-设置-深色",
    "11-空状态-首次使用",
    "12-空状态-搜索无结果",
    "13-加载骨架屏",
]

# 「Logo 品牌资产」不是 13 屏之一，而是 board 里第 7 块（下标 6 的那段说明版）。
# 它本来就不是画板、没有 .caption 结构，所以单独走一条取法。
LOGO_SHEET = "14-Logo品牌资产"
LOGO_BOARD_INDEX = 6

# 前 6 项是画板（5 个 .pair + 1 个 .grid3），总计 2+2+2+2+2+3 = 13 屏
HARVEST = """
/* 从原型内存里的 board 重新取屏：前 6 项是 13 个画板，第 7 项起是说明文字 */
(function () {
  const cells = [];
  board.slice(0, 6).forEach(function (block) {
    // 两级：包裹 div → .pair / .grid3 容器 → 每格一个画板
    const box = new DOMParser().parseFromString("<div>" + block + "</div>", "text/html")
      .body.firstElementChild;
    const group = box.firstElementChild;   // .pair 或 .grid3
    Array.prototype.forEach.call(group.children, function (cell) {
      const cap = cell.querySelector(".caption");
      if (cap) cap.remove();          // 去掉「01 主界面 · 浅色」这类说明文字
      cells.push(cell.innerHTML.trim());
    });
  });
  if (cells.length !== %(total)d) {
    document.title = "屏数不符: " + cells.length;
    console.error("期望 %(total)d 屏，实际 " + cells.length + " 屏");
  }
  document.getElementById("root").innerHTML = cells[%(index)d] || "<p>取屏失败</p>";
  // 取屏完成后清掉临时落点，否则导入设计工具时会把 13 屏的隐藏副本也转成图层
  document.getElementById("board").remove();
})();
"""

# Logo 板：直接把 board 的第 7 块（Logo 定稿）搬过来
HARVEST_LOGO = """
(function () {
  document.getElementById("root").innerHTML = board[%(index)d] || "<p>取屏失败</p>";
  document.getElementById("board").remove();
})();
"""

TEMPLATE = """<!doctype html>
<html lang="zh-CN"><head><meta charset="utf-8">
<title>%(title)s</title>
<style>
%(css)s
</style>
<style>
/* ---- Ardot 导入用的覆盖样式 ---- */
html, body { margin: 0; padding: 0; background: #fff; overflow: hidden; }

/* 去掉网页般的圆角与投影，让每屏是一个干净的矩形画板 */
.window { border-radius: 0 !important; box-shadow: none !important; }

/* 标题栏交给 Ardot 自己表达，内容面直接用满 600px */
.window__bar { display: none !important; }
.window__body { height: 600px !important; }

/* 列表区吃满剩余高度，状态栏才能贴住底边 */
.list-region { flex: 1; min-height: 0; overflow: hidden; }

/* 取屏用的隐藏落点，不参与导出 */
#board { position: absolute; left: -99999px; top: 0; width: 1px; height: 1px; overflow: hidden; }
</style>
</head><body>
<div id="root"></div>
<div id="board"></div>
<script>
%(lib)s
%(harvest)s
</script>
</body></html>
"""


def main() -> int:
    outdir = sys.argv[1] if len(sys.argv) > 1 else "/tmp/ardot_html"
    os.makedirs(outdir, exist_ok=True)

    src = open(os.path.join(PROTO_DIR, "prototype.html"), encoding="utf-8").read()
    css = open(os.path.join(PROTO_DIR, "proto.css"), encoding="utf-8").read()

    # 整段原型脚本原样保留（含末尾把 board 写进 DOM 的那句）
    lib = src.split("<script>", 1)[1].rsplit("</script>", 1)[0]
    # 原脚本末尾会写 #board —— 那是我们的隐藏落点，正是需要的；后面再接取屏逻辑
    assert 'document.getElementById("board")' in lib, "原型脚本结构变了：找不到 #board 落点"

    total = len(SCREENS)
    for index, name in enumerate(SCREENS):
        html = TEMPLATE % {
            "title": name,
            "css": css,
            "lib": lib,
            "harvest": HARVEST % {"total": total, "index": index},
        }
        path = os.path.join(outdir, f"{name}.html")
        with open(path, "w", encoding="utf-8") as f:
            f.write(html)
        print(f"{os.path.getsize(path) // 1024:>3} KB  {name}.html")

    # Logo 品牌资产板
    html = TEMPLATE % {
        "title": LOGO_SHEET,
        "css": css,
        "lib": lib,
        "harvest": HARVEST_LOGO % {"index": LOGO_BOARD_INDEX},
    }
    path = os.path.join(outdir, f"{LOGO_SHEET}.html")
    with open(path, "w", encoding="utf-8") as f:
        f.write(html)
    print(f"{os.path.getsize(path) // 1024:>3} KB  {LOGO_SHEET}.html")

    print(f"\n共 {total} 个单屏 + 1 个 Logo 板 → {outdir}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
