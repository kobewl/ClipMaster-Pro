# Dock 图标"变大"的根因与修正

> 2026-09-17 · 症状：Dock 里的 ClipMaster 图标比邻居明显大一圈
> **状态：已修复并重新生成全部图标**

## 结论先说

**不是图标"被放大"了，是新图标把画布铺满了。**

Dock 给每个图标分配的格子是固定大小的。图标文件里图形占画布的比例越大，
在 Dock 里看起来就越大。macOS 应用的惯例是图形只占画布 **约 80%**，
四周留一圈透明——这一圈透明不是浪费，是让所有图标"视觉等重"的对齐基准。

| | 图形占画布 | 圆角/边长 |
|---|---|---|
| **Apple 自家图标**（Safari/备忘录/计算器/邮件/音乐…共 7 个实测） | **79.7%** | **21.8%** |
| Window 版常见第三方图标（ChatGPT / Claude） | 84.6 ~ 86.1% | — |
| **ClipMaster 修复前** | **100%** | **29.3%** |
| ClipMaster 修复后 | **79.7%** | **21.6%** |
| ClipMaster 更早的旧 Logo（git HEAD） | 73.1% | 45.7% |

图形占 100%，比 Apple 的基准大了 **25%**——Dock 里一眼就看出来。
圆角也比标准大了一档，所以形状显得更"方胖"。

---

## 修复方式

**只改了 `public/logo.svg` 的两行，图形坐标一个都没动：**

```diff
- <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512">
-   <rect width="512" height="512" rx="150" fill="#5B5FEF"/>
+ <svg xmlns="http://www.w3.org/2000/svg" viewBox="-64 -64 640 640">
+   <rect width="512" height="512" rx="112" fill="#5B5FEF"/>
```

原理：

- **viewBox 外扩到 640×640**（往四周各扩 64）：512 的图形自然落在画布中间，
  占 `512/640 = 80.0%` —— 正好对齐 Apple 的 79.7%。
- **`rx` 从 150 收到 112**：圆角比 `112/512 = 21.9%` —— 对齐 Apple 的 21.8%。

### 重新生成

```bash
npx tauri icon public/logo.svg
```

一次生成 macOS（`.icns` / `.png` 多尺寸）、Windows（`.ico` / Square 系列）、
iOS、Android 全部图标。

favicon（`public/logo.png`，被 `index.html` 引用）不在 `tauri icon` 的范围内，
需要用同一个 SVG 单独导出 512×512 透明 PNG。

### 实测结果

```
icon.png               画布  512  形状 408px = 79.7%   圆角 21.3%   留白 10.2%
128x128.png            画布  128  形状 102px = 79.7%   圆角 21.6%   留白 10.2%
128x128@2x.png         画布  256  形状 204px = 79.7%   圆角 21.6%   留白 10.2%
icon.icns (最大帧)      画布 1024  形状 818px = 79.9%   圆角 21.6%   留白 10.1%
Square150x150Logo.png  画布  150  形状 120px = 80.0%   圆角 22.5%   留白 10.0%
public/logo.png (favicon) 画布 512  形状 79.7%          圆角 21.3%   留白 10.2%
```

**全部对齐 Apple 基准。**（`32x32.png` 是 75.0%，那是栅格化到 32px 时的
取整误差，符合预期。）

---

## 改了哪些文件

| 文件 | 改动 |
|---|---|
| `public/logo.svg` | viewBox 外扩 + `rx` 150→112（**就这两处**） |
| `public/logo.png` | 用新 SVG 重新导出 512×512（favicon） |
| `src-tauri/icons/*` | `npx tauri icon` 重新生成的全部平台图标 |

**`docs/design/logo/` 里那批设计稿（`clipmaster-logo.svg` 等）未改动** ——
它们是给设计工具用的展示版，底板铺满画布在那里是对的（设计稿本来就该画满
画板，边距是导出成 App 图标时才需要考虑的事）。

---

## 另一个要紧的事：`public/logo.png` 和 `logo.svg` 的一致性

- `public/logo.svg` —— 图标**源文件**（改的是它）
- `public/logo.png` —— favicon（被 `index.html` 引用）
- `src-tauri/icons/*` —— `tauri icon` 从 `logo.svg` 生成的**产物**

三个都更新过了。以后再改 Logo，**只需要动 `logo.svg`，然后重跑
`npx tauri icon public/logo.svg` 并重新导出 favicon**。

---

## 附：Apple 基准的实测数据

```
Apple 自家图标（/System/Applications/*.app/Contents/Resources/*.icns，256×256 帧）
  Safari      图形 204px = 79.7%   圆角 44.5px = 21.8%
  Notes       图形 204px = 79.7%   圆角 44.5px = 21.8%
  Calculator  图形 204px = 79.7%   圆角 44.5px = 21.8%
  Terminal    图形 204px = 79.7%   圆角 44.5px = 21.8%
  Mail        图形 204px = 79.7%   圆角 44.5px = 21.8%
  Preview     图形 204px = 79.7%   圆角 44.5px = 21.8%
  Photos      图形 204px = 79.7%   圆角 44.5px = 21.8%
  Music       图形 204px = 79.7%   圆角 44.5px = 21.8%

第三方（/Applications）
  ChatGPT     图形 881px = 86.1%（四边留白 6.9/7.9/6.9/6.0%）
  Claude      图形 866px = 84.6%
```

（Apple 那 7 个图标数值完全一致，说明这是系统级的统一规范，不是巧合。）

---

## 怎么复测

```bash
python3 - <<'EOF'
from PIL import Image
import numpy as np
im = Image.open("src-tauri/icons/icon.png").convert("RGBA")
a = np.array(im.getchannel("A"))
ys, xs = np.where(a > 250)
w = a.shape[1]
print(f"图形占画布 {(xs.max()-xs.min()+1)/w*100:.1f}%  (目标 79.7%)")
print(f"四边留白 {xs.min()/w*100:.1f}%              (目标 10.2%)")
EOF
```

改了 Logo 之后跑一遍，两个数都应该落在 79.7% / 10.2% 附近。
