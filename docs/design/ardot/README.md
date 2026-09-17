# 在 Ardot 里看设计稿

设计稿已导入腾讯 **Ardot**（在线协作设计工具），可以在浏览器里打开、拖动、修改：

**https://ardot.tencent.com/file/726701811252428**

---

## 画布上有什么

画布是「13 屏界面 + 1 块 Logo 板」共 14 个画板，排成 4 列网格：

| 位置 | 内容 |
|---|---|
| 第 1 行 | 01 主界面·浅色 · 02 主界面·深色 · 03 搜索命中 · 04 行悬停 |
| 第 2 行 | 05 分组菜单 · 06 图片记录 · 07 偏好设置 · 08 管理分组 |
| 第 3 行 | 09 危险操作确认 · 10 偏好设置·深色 · 11 空状态·首次使用 · 12 空状态·搜索无结果 |
| 第 4 行 | 13 加载骨架屏 · （空三格） |
| 第 5 行 | 14 Logo · 品牌资产 |

每个画板都是 **800×600**，和 `tauri.conf.json` 里的窗口尺寸一致。
所有元素都是**可编辑图层**（Frame / Rectangle / Text），不是一张图片 ——
点进去就能改文字、换颜色、量间距。

---

## 怎么改

在 Ardot 网页里直接操作即可，以下几点说明为什么这样组织：

- **颜色**照 `../tokens.json` 的 `semantic` 段改。画布上的色值已经按令牌刷过一遍，
  改一处颜色时建议全局替换，别单点改。
- **文字**统一用 Noto Sans SC（界面文案）+ Inter（数字/英文）。
  Ardot 里没有系统字体苹方，Noto Sans SC 是最接近的替代，行高表现一致。
- **打开某个画板单独改**：双击进入画板，改完按 Esc 退出。
  想复制一屏做变体，选中画板后 Cmd+D。

---

## 怎么重新导入

改完 HTML 原型后，用这两步把新版本推回 Ardot：

```bash
cd docs/design/ardot

# 1. 从 prototype.html 重新生成 14 个自包含单屏 HTML
python3 build_ardot_html.py /tmp/ardot_html

# 2. 逐屏导入 Ardot（图片会自动上传，不再需要手工准备）
python3 import_screens.py

# 只导入某几屏（按文件名里的编号匹配）
python3 import_screens.py 03 07
```

### 依赖

- `ardot_client.py` 负责与 Ardot 通信。它从 `~/.zcode/v2/credentials.json`
  读出 OAuth 令牌（AES-256-GCM 解密，密钥派生方式与 ZCode 一致），
  再直接发 MCP JSON-RPC 请求。**首次使用前需要先在 ZCode 的 设置 → MCP
  里完成 `ardot-remote` 授权**，令牌有效期 15 天，过期后重新授权即可。
- 需要 `cryptography` 包（`pip3 install cryptography`）。

### 单独调工具

```bash
# 列出 Ardot 的 26 个设计工具
python3 ardot_client.py tools

# 直接调某个工具
python3 ardot_client.py call fetch_editor_state '{"fileUrl":"https://ardot.tencent.com/file/726701811252428"}'

# 上传一张图，拿到可用于其它工具的公网地址
python3 ardot_client.py upload ./logo.png image/png https://ardot.tencent.com/file/726701811252428
```

---

## 三个已知的渲染差异

Ardot 的渲染引擎和浏览器不是同一个，以下是实测确认的差异，**代码侧不受影响**：

1. **Logo 的投影会丢失。** `clipmaster-logo.svg` 里那道「纸张投在底板上的阴影」
   用了 SVG 滤镜 `feDropShadow`，Ardot 不支持滤镜，导入后纸张会平贴在底板上，
   少了「浮起来」的感觉。浏览器、Tauri WebView、macOS 图标生成都完整支持。
   ——需要不带滤镜的版本时用 `clipmaster-logo-flat.svg`（纯色块模拟投影）。

2. **图片必须是公网地址。** 原型里的示例缩略图原本是内联 data URI，
   Ardot 是服务端拉取 HTML 渲染的，读不到 data URI。
   `import_screens.py` 会把它们自动换成 Ardot 的临时上传地址。

3. **中文字体降级为 Noto Sans SC。** 原型里用的系统苹方 Ardot 没有；
   换字体后字宽略有变化，但字阶、行高、留白全部一致。

---

## 文件说明

| 文件 | 作用 |
|---|---|
| `build_ardot_html.py` | 把 `prototype.html` 的 13 屏 + Logo 板拆成 14 个自包含单屏 HTML |
| `import_screens.py` | 逐屏上传并导入 Ardot；自动处理图片外链 |
| `ardot_client.py` | Ardot MCP 客户端（凭据解密 / JSON-RPC / 资产上传），可单独当命令行用 |

中间产物（可随时删）：`/tmp/ardot_html/`、`/tmp/ardot_assets/`。
