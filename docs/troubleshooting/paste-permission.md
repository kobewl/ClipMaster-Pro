# 一键粘贴报错排查：`不允许发送按键 (1002)`

> 2026-09-17 · 症状：点历史记录粘贴时弹出
> `模拟粘贴失败: 36:68: execution error: "System Events"遇到一个错误："osascript"不允许发送按键。(1002)`

## 一句话结论

粘贴一直是**靠 `osascript` 指挥 System Events 去按键**实现的，这条路要过两道 macOS 权限；
`osascript` 那一道被拒了。现在改成 **CoreGraphics 直接投递按键**，只需要一道权限，
而且能在发送前就查出来，提示不再是天书。

---

## 为什么原来是坏的

### 两道权限，缺一不可

旧实现跑的是这条命令：

```bash
osascript -e 'tell application "System Events" to keystroke "v" using command down'
```

它要同时满足两件事：

| # | 权限 | 用途 |
|---|---|---|
| 1 | **自动化**（Automation / AppleEvents） | 让 ClipMaster 有权「指挥」System Events |
| 2 | **辅助功能**（Accessibility） | 让 System Events 有权「真的按键」 |

**第 1 道被拒了**，错误码 `1002`（`errAEEventNotPermitted`），所以报错里的主语是
`"osascript"不允许发送按键` —— 注意是 osascript 被拒，不是 ClipMaster 被拒。
用户看到这句话根本不知道自己该去开哪个开关。

### 更糟的是：这段代码本来就抓不住这个错误

`paste_error_hint()` 只认这些关键词：

```rust
["not allowed", "not authorized", "-1719", "-1743", "keystrokes"]
```

而这次系统返回的是**中文文案 + 新错误码**：

```
"osascript"不允许发送按键。(1002)
```

- 文案是中文的 → `not allowed` / `keystrokes` 都匹配不上
- 错误码是 `1002` → `-1719` / `-1743` 也匹配不上

于是本该显示的友好提示（「请到系统设置里勾选…」）**从来没出现过**，
用户看到的一直是系统原文。这是代码里一个真实缺陷。

---

## 现在的实现

### 换成 CoreGraphics 投递按键

```rust
let source = CGEventSource::new(CGEventSourceStateID::CombinedSessionState);
let down = CGEvent::new_keyboard_event(source_ref, KEY_V, true)?;
let up   = CGEvent::new_keyboard_event(source_ref, KEY_V, false)?;

// 修饰键挂在事件本身上，而不是单独发一个 Command 键——
// 后者在目标应用看来是「按下了 Command」，偶尔会被当成粘滞键处理。
CGEvent::set_flags(Some(&down), CGEventFlags::MaskCommand);
CGEvent::set_flags(Some(&up),   CGEventFlags::MaskCommand);

CGEvent::post(CGEventTapLocation::HIDEventTap, Some(&down));
CGEvent::post(CGEventTapLocation::HIDEventTap, Some(&up));
```

两个关键选择：

- **`HIDEventTap`**（而不是 `SessionEventTap`）：这是系统输入流的最上端，
  等价于真实键盘，能送达的范围最大。
- **虚拟键码 `0x09`**（`kVK_ANSI_V`）对应键盘上的**物理位置**，
  和输入法、键盘布局无关。所以中文/日文/Dvorak 布局下都是同一个键。

### 发送前先查权限

```rust
if !has_accessibility_permission() {
    open_accessibility_settings();      // 直接把设置面板送到他面前
    return Err(MISSING_ACCESSIBILITY_HINT.to_string());
}
```

`AXIsProcessTrusted()` 在 `ApplicationServices` 框架里，和 AppKit 一样是系统自带的，
用一句 `#[link]` + `extern "C"` 就能声明，不用引新依赖。

> 返回类型是 C 的 `Boolean`（`unsigned char`），不是 Rust 的 `bool`，所以用 `u8` 接。

### 顺手做了三件事

1. **自动打开授权页**：用户已经点了「粘贴」，意图很明确，
   与其让他照着提示自己找三层菜单，不如直接跳转。
   用的是 URL scheme，比 `AXIsProcessTrustedWithOptions(prompt: true)` 少写一大截
   （后者要构造 CFDictionary、处理 CFString 外链常量），效果一样。

2. **修了一个竞态**：`activateWithOptions`（归还焦点）是**异步**的，
   紧接着投递按键会打空 —— 事件落在一个正在失去焦点的窗口上。
   现在归还焦点后等 80ms 再发。

3. **错误文案重写**：新提示直接说清「去哪个面板、打开哪个开关、
   为什么开关打开后还要重启」。

---

## 你现在需要做什么

### 1. 重新启动应用

```bash
npm run tauri dev
```

代码已经改好并编译通过了（12 个测试全过、零警告），重启就能用上新实现。

### 2. 试试粘贴

召唤窗口（`⌘⇧V`）→ 点一条记录。**大概率直接就好了**，原因见下一节。

### 3. 如果还不行

应用会**自动帮你打开**「系统设置 → 隐私与安全性 → 辅助功能」，
把 **ClipMaster Pro** 的开关打开，然后**重启应用**（macOS 只在进程启动时读一次权限）。

---

## dev 模式下的权限继承（实测结论）

排查时用两种启动方式做了对照实验，结果很关键：

| 启动方式 | 父进程 | 辅助功能权限 |
|---|---|---|
| 从终端启动 | `/bin/zsh` | **有** |
| 脱离终端启动（launchd） | `/sbin/launchd` | **无** |

**权限是从父进程继承的，不是二进制自带的。**

跑 `npm run tauri dev` 时，进程链是：

```
zsh → npm run tauri dev → node tauri dev → clipmaster-pro
```

所以 **dev 模式下的 ClipMaster 会继承那个终端的辅助功能权限** ——
只要你的终端（Cursor / ZCode 的内置终端）已经有辅助功能权限，
应用就自动有，不用在设置里单独勾选。

> 这也是为什么旧代码用 osascript 会失败：osascript 是**另一个进程**，
> 它要的是「自动化」权限，而那个权限**不能继承**，必须单独授予。

打包成正式 `.app` 后（`npm run tauri build`）就没有这个继承了，
用户首次使用需要自己授权一次 —— 这是正常且预期的。

---

## 一个 dev 模式的坑（次要）

实测发现：**每次重新编译，dev 二进制的签名都会变**。

```
编译前  CDHash=39fd37bcf5eb2fe9a768fc6b175da235448c6c43
重链接后 CDHash=0973533caf5a841ab554b6ea2a0cbef154c7b246
```

原因是 Tauri dev 产出的二进制是 **ad-hoc 签名**（`Signature=adhoc`，
`TeamIdentifier=not set`），本机也没有任何代码签名证书（`security find-identity`
返回 0 个）。ad-hoc 签名没有稳定身份，macOS 的 TCC 只能用 CDHash 来认它 ——
CDHash 一变，系统就认为这是「另一个应用」，之前单独授予它的权限就失效了。

**只有在「你手动给 ClipMaster Pro 勾了开关」的情况下才会碰到这个坑**
（走继承就绕过了）。表现是：设置里那个开关**看着还是开着的**
（那是旧 CDHash 的记录），但实际不生效 —— 最容易让人以为「修复没用」。

**应对**：

| 做法 | 说明 |
|---|---|
| 重新授权 | 把开关**先关掉再打开**，然后重启应用 |
| 打包成正式 .app 再测 | `npm run tauri build`，产物有稳定的 bundle 身份 |
| 建一个自签名证书 | 给 dev 二进制稳定身份，一次授权长期有效 |

> 需要的话可以帮你建自签名证书，一劳永逸。目前 dev 阶段靠权限继承，
> 所以通常碰不到这个问题。

---

## 改了哪些文件

| 文件 | 改动 |
|---|---|
| `src-tauri/Cargo.toml` | 新增 `objc2-core-graphics` 依赖（`CGEvent` / `CGEventSource` / `CGEventTypes` / `CGRemoteOperation` 四个 feature） |
| `src-tauri/Cargo.lock` | 自动更新 |
| `src-tauri/src/infrastructure/appicon/macos.rs` | `simulate_paste()` 改为 CGEvent；新增 `has_accessibility_permission()`、`post_command_v()`、`open_accessibility_settings()`；重写错误文案；新增 2 个单元测试 |

**没有引入新的下载量**：`objc2-core-graphics` 本来就在 Tauri 的依赖树里（
`objc2-app-kit` 间接依赖它），这次只是显式引用。

---

## 怎么验证修好了

```bash
cd src-tauri
cargo test --lib        # 12 个测试，含 2 个新增的
cargo build             # 应当零警告
```

端到端验证（已实测通过，真实粘贴成功）：

1. 复制一段文字
2. 唤起 ClipMaster（`⌘⇧V`）
3. 点那条记录
4. 内容应当粘进刚才的应用

实测记录：剪贴板放 `PASTE-TEST-110030` → 调用 `simulate_paste()` →
TextEdit 文档内容变成 `PASTE-TEST-110030` ✓

**如果还是失败**，现在的提示会明确告诉你缺什么，而不是一段天书。
