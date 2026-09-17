# ClipMaster Pro

<div align="center">
  <h3>轻量级、本地优先、隐私友好的 macOS 剪贴板助手</h3>
</div>

## 项目状态

ClipMaster Pro 正在进行架构重构：原 Python/PyQt6 实现（v2）已被移除，不再作为迁移数据源或行为参照。当前唯一在开发的实现是 Tauri 2 + Rust + React/TypeScript 应用，目标版本为 **0.01 Beta**，首发平台为 macOS。

详细的产品需求、架构设计、研发计划和治理规范见项目文档中心（重构文档目录），本 README 只覆盖如何构建和运行当前代码。

## 功能

**已有**

- 剪贴板历史自动采集（文本 / 图片），按内容指纹去重
- 全局快捷键唤起，一键粘贴回刚才那个应用
- 全文搜索（SQLite FTS5，中文分词友好）
- 分组管理（自定义名称与颜色）
- 来源应用识别，并显示该应用的**真实图标**
- 数据保留策略（最大条数 / 保留天数），暂停采集
- **查看全部内容**：长文本与图片可弹窗看全，文字可拖选
- **图片放大**：预览里点图切换 100% / 适应窗口，`⌘` 滚轮连续缩放，可拖动平移
- **多选合并复制**：勾选多条按顺序合并成一段文本，一次复制或粘贴
- **开机自启**：登录后安静驻留后台，不弹窗

**规划中**

- 完整应用 `docs/design/` 里定稿的新版视觉（当前仍是旧版界面，见 `docs/USAGE.md`）
- 代码签名与自动更新
- 菜单栏图标

## 使用说明

见 [docs/USAGE.md](docs/USAGE.md)（快捷键、权限授权、数据位置、已知问题）。
界面设计规范见 [docs/design/README.md](docs/design/README.md)。

## 技术栈

- 桌面框架：Tauri 2
- 前端：React + TypeScript + Vite + Tailwind CSS
- 后端：Rust + Tokio
- 数据库：SQLite（rusqlite）
- macOS 剪贴板：clipboard-rs

## 目录结构

```text
ClipMaster-Pro/
├── src/                    # 前端 React 代码
├── src-tauri/               # Rust 后端
│   └── src/
│       ├── domain/              # 领域模型、错误、端口接口
│       ├── application/         # 用例编排
│       ├── infrastructure/      # SQLite、macOS 剪贴板适配器
│       ├── commands/            # Tauri Command/DTO
│       └── lifecycle/           # Composition Root
├── package.json
├── LICENSE
└── README.md
```

## 环境要求

- Node.js 20+
- Rust（stable，通过 rustup 安装）
- macOS 13+（Apple Silicon 优先，Intel 兼顾）
- Xcode Command Line Tools（`xcode-select --install`）

## 本地开发

```bash
npm install
npm run tauri dev
```

## 构建

```bash
npm run build        # 仅前端类型检查 + 构建
npx tauri build       # 生成 macOS 应用产物
```

## 测试

```bash
cd src-tauri
cargo test
cargo clippy --all-targets
```

## 许可证

本项目采用 MIT 许可证。详情请参阅 [LICENSE](LICENSE) 文件。
