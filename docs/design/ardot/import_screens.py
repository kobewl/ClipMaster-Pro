#!/usr/bin/env python3
"""
把 13 屏原型导入 Ardot，形成可编辑图层。

流程（每一屏独立走一遍）：
  1. 读单屏 HTML，把里面的 data URI 图片换成 Ardot 公网临时地址
     —— Ardot 是服务端拉取 HTML 渲染的，data URI 拿不到图，必须换外链。
  2. 申请上传地址 → PUT 上传 → 拿到 downloadUrl。
  3. html_to_ardot(htmlUrl=downloadUrl, viewport=800) 让它转成图层。

用法：
    python3 import_screens.py            # 导入全部 13 屏
    python3 import_screens.py 03 07      # 只导入名字里含 03 / 07 的屏
"""

import json
import os
import re
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from ardot_client import call, register_asset  # noqa: E402

import urllib.request  # noqa: E402

FILE_URL = "https://ardot.tencent.com/file/726701811252428"
PAGE_ID = "0:1"
HTML_DIR = "/tmp/ardot_html"
ASSET_DIR = "/tmp/ardot_assets"

# data URI → 本地 PNG 资产的对应关系（都在 prototype.html 的 IMG / IMG2 里定义）
DATA_URI_ASSETS = [
    ("thumb-gradient.png", "#6366F1"),   # 蓝紫→青 渐变缩略图
    ("thumb-window.png",   "#1b1d26"),   # 深色窗口截图缩略图
]


def put(url: str, data: bytes, mime: str) -> None:
    req = urllib.request.Request(url, data=data, method="PUT",
                                headers={"Content-Type": mime})
    with urllib.request.urlopen(req, timeout=180) as r:
        if r.status not in (200, 201, 204):
            raise SystemExit(f"上传失败 HTTP {r.status}")


def upload_asset(path: str, mime: str) -> str:
    slot = register_asset(FILE_URL, mime)
    put(slot["uploadUrl"], open(path, "rb").read(), mime)
    return slot["downloadUrl"]


def rewrite_data_uris(html: str, urls: dict) -> tuple[str, int]:
    """
    把 HTML 里的示例缩略图换成上传后的公网地址。

    原型里的写法是「字面量 + encodeURIComponent(模板字符串)」两段拼出来的：
        const IMG = "data:image/svg+xml;utf8," + encodeURIComponent(`<svg …>`);
    所以没法按完整的 data URI 去匹配，要连拼接表达式整体替换成一个普通 URL 字面量。
    两张缩略图按 SVG 里的特征色区分。

    urls 由调用方预先上传好并复用 —— 临时地址有效期约 16 分钟，一批导入够用。
    """
    # 匹配 IMG / IMG2 的整条定义：从 data URI 字面量到 encodeURIComponent(...) 结束
    pattern = re.compile(
        r'"data:image/svg\+xml;utf8,"\s*\+\s*encodeURIComponent\(`.*?`\)',
        re.DOTALL,
    )

    count = 0

    def repl(m: re.Match) -> str:
        nonlocal count
        for marker, url in urls.items():
            if marker.lower() in m.group(0).lower():
                count += 1
                return json.dumps(url)
        return m.group(0)

    return pattern.sub(repl, html), count


def upload_thumbnails() -> dict:
    """上传两张示例缩略图，返回 {特征色: 公网地址}。"""
    urls = {}
    for fname, marker in DATA_URI_ASSETS:
        p = os.path.join(ASSET_DIR, fname)
        if not os.path.exists(p):
            raise SystemExit(f"缺少资产 {p}")
        urls[marker] = upload_asset(p, "image/png")
    return urls


def import_screen(name: str, thumb_urls: dict) -> dict:
    path = os.path.join(HTML_DIR, f"{name}.html")
    html = open(path, encoding="utf-8").read()

    html, replaced = rewrite_data_uris(html, thumb_urls)
    tmp = os.path.join("/tmp", f"_up_{name}.html")
    open(tmp, "w", encoding="utf-8").write(html)

    html_url = upload_asset(tmp, "text/html")
    r = call("html_to_ardot", {
        "fileUrl": FILE_URL,
        "htmlUrl": html_url,
        "pageId": PAGE_ID,
        "viewport": 800,
        "theme": "light",
    })
    ok = isinstance(r, dict) and r.get("success") and r.get("data", {}).get("success")
    return {"screen": name, "ok": ok, "images": replaced, "result": r}


def main() -> int:
    filters = sys.argv[1:]
    names = sorted(f[:-5] for f in os.listdir(HTML_DIR)
                   if f.endswith(".html") and not f.startswith("_"))
    if filters:
        names = [n for n in names if any(f in n for f in filters)]

    print(f"准备导入 {len(names)} 屏")
    thumb_urls = upload_thumbnails()
    print(f"缩略图已上传（{len(thumb_urls)} 张），开始逐屏导入\n")

    out = []
    for i, n in enumerate(names, 1):
        t0 = time.time()
        try:
            r = import_screen(n, thumb_urls)
            flag = "✓" if r["ok"] else "✗"
            print(f"{flag} [{i:>2}/{len(names)}] {n}  "
                  f"图片 {r['images']} 张  {time.time() - t0:.1f}s")
            if not r["ok"]:
                print("   ", json.dumps(r["result"], ensure_ascii=False)[:400])
        except Exception as e:
            print(f"✗ [{i:>2}/{len(names)}] {n}  异常: {e}")
            r = {"screen": n, "ok": False, "error": str(e)}
        out.append(r)

    json.dump(out, open("/tmp/ardot_import_result.json", "w"),
              ensure_ascii=False, indent=1)
    ok = sum(1 for r in out if r.get("ok"))
    print(f"\n完成：{ok}/{len(names)} 屏导入成功")
    return 0 if ok == len(names) else 1


if __name__ == "__main__":
    raise SystemExit(main())
