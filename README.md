# ClipMaster Pro

<div align="center">
  <h3>轻量级、本地优先、隐私友好的 macOS 剪贴板助手</h3>
</div>

## 项目状态

ClipMaster Pro 正在进行架构重构：原 Python/PyQt6 实现（v2）已被移除，不再作为迁移数据源或行为参照。当前唯一在开发的实现是 Tauri 2 + Rust + React/TypeScript 应用，目标版本为 **0.01 Beta**，首发平台为 macOS。

详细的产品需求、架构设计、研发计划和治理规范见项目文档中心（重构文档目录），本 README 只覆盖如何构建和运行当前代码。

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
