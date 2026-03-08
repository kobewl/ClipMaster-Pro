# ClipMaster Pro v2.0

<div align="center">
  <h3>🚀 现代化、高性能的剪贴板管理工具</h3>
  <p>专为效率而生，让复制粘贴更智能</p>
</div>

## ✨ 新特性 (v2.0)

### 🎯 性能优化
- **SQLite 数据库存储** - 支持数万条记录流畅运行
- **智能防抖机制** - 剪贴板监听更稳定，避免重复记录
- **数据压缩** - 自动压缩大文本，节省存储空间
- **虚拟滚动** - 大数据量列表不卡顿
- **延迟加载** - 快速启动，按需加载历史记录

### 🎨 全新UI设计
- **现代化界面** - 采用当下流行的设计语言
- **圆角窗口** - 更柔和的视觉效果
- **平滑动画** - 流畅的交互体验
- **高DPI支持** - 完美适配高分屏
- **自定义滚动条** - 更美观的滚动体验

### 📋 增强功能
- **多格式支持** - 文本、图片、文件、HTML
- **JetBrains 兼容修复** - 修复 IntelliJ IDEA / PyCharm 等工具复制内容无法进入历史记录的问题
- **收藏功能** - 收藏重要内容，快速访问
- **智能搜索** - 支持模糊搜索，快速定位
- **内容预览** - 列表显示内容预览和时间戳
- **类型图标** - 不同内容类型用不同图标标识

### ⚙️ 高级设置
- **自动清理** - 自动清理过期历史记录
- **保留收藏** - 清理时保留收藏的项目
- **开机启动** - 支持开机自启动
- **全局热键** - 可自定义快捷键（支持双模式注册）
- **热键冲突检测** - 自动检测并提示热键被占用情况
- **数据导入导出** - JSON格式备份和恢复

## 📦 安装说明

### 环境要求
- Python 3.10 或更高版本
- Windows 10/11
- macOS 13+（Apple Silicon / Intel，已支持基础使用、全局热键、窗口置顶）

### 安装步骤

1. 克隆仓库到本地
```bash
git clone https://github.com/kobewl/ClipMaster-Pro.git
cd ClipMaster-Pro
```

2. 安装依赖包
```bash
pip install -r requirements.txt
```

3. 运行应用程序
```bash
python src/main.py
```

### macOS 运行说明

推荐使用虚拟环境：

```bash
python3 -m venv .venv
.venv/bin/python -m pip install -r requirements.txt
.venv/bin/python src/main.py
```

如果你要使用全局热键和 AI 输入监听，需要在 macOS 中授予当前运行应用的辅助功能权限：

1. 打开 `系统设置 -> 隐私与安全性 -> 辅助功能`
2. 给 `Terminal`、`iTerm` 或你实际运行 ClipMaster Pro 的宿主应用开启权限
3. 重新启动 ClipMaster Pro

### macOS 打包

项目内置了 macOS 打包脚本，可直接生成 `.app`：

```bash
.venv/bin/python build_macos.py
```

打包完成后产物位于：

```bash
dist/ClipMasterPro.app
```

## 🚀 使用指南

### 基本操作

| 操作 | 说明 |
|------|------|
| **Ctrl + O** | 显示/隐藏主窗口（可自定义） |
| **Ctrl + Shift + C** | 清空历史记录（可自定义） |
| **Ctrl + F** | 聚焦搜索框（可自定义） |
| **ESC** | 隐藏窗口 |
| **点击项目** | 复制到剪贴板 |
| **双击项目** | 查看完整内容 |
| **右键项目** | 打开操作菜单 |

macOS 默认热键：

| 操作 | 说明 |
|------|------|
| **Command + `** | 显示/隐藏主窗口 |
| **Command + Shift + C** | 清空历史记录 |
| **Command + F** | 聚焦搜索框 |

### 自定义热键

1. 点击主窗口右上角的 **⚙️ 设置** 按钮
2. 切换到 **"热键"** 选项卡
3. 点击要修改的热键旁边的 **"设置热键"** 按钮
4. 按钮变为 **"捕获中..."**，此时按下你想要的快捷键组合
5. 热键会被自动捕获并显示
6. 点击 **"确定"** 保存设置

**热键设置说明：**
- 热键必须包含至少一个修饰键（Ctrl、Alt、Shift、Win / Command）
- 支持字母、数字、F1-F12、符号键等
- 如果热键被其他程序占用，会自动使用备用模式注册
- 推荐避免与常用软件（如截图工具、IDE）冲突的热键组合
- macOS 上热键依赖辅助功能权限；未授权时日志会提示监听不可用

### 界面说明

```
┌─────────────────────────────────────┐
│ 📋 ClipMaster Pro    📌 ⚙️ 🌙 ✕    │  ← 标题栏（可拖动）
├─────────────────────────────────────┤
│ 🔍 搜索...              [⭐收藏] 📊 │  ← 搜索和工具栏
├─────────────────────────────────────┤
│ 📝 14:32 | 这是复制的内容...    ☆   │  ← 历史记录列表
│ 🖼️ 14:30 | [图片] 1920x1080     ☆   │
│ 📎 14:28 | [文件] document.pdf  ⭐   │  ← 收藏的项目
│ ...                                 │
├─────────────────────────────────────┤
│ 共 128 条记录                       │  ← 状态栏
└─────────────────────────────────────┘
```

### 托盘菜单
右键点击系统托盘图标可访问：
- 显示主窗口
- 清空历史记录
- 数据管理
- 切换主题
- 设置
- 退出

## 🏗️ 项目结构

```
ClipMaster-Pro/
├── src/                          # 源代码目录
│   ├── config/                   # 配置管理
│   │   ├── __init__.py
│   │   └── settings.py          # 设置管理（线程安全）
│   ├── controllers/              # 控制器层
│   │   ├── __init__.py
│   │   ├── clipboard_controller.py   # 剪贴板控制
│   │   ├── hotkey_controller.py      # 热键管理（Windows: RegisterHotKey；macOS: pynput）
│   │   └── input_monitor.py          # 输入监控（AI预测用）
│   ├── models/                   # 数据模型层
│   │   ├── __init__.py
│   │   └── clipboard_item.py    # 剪贴板项模型
│   ├── services/                 # 服务层
│   │   ├── __init__.py
│   │   ├── clipboard_service.py # 核心业务逻辑
│   │   ├── ai_service.py        # AI服务
│   │   └── prediction_engine.py # 智能预测引擎
│   ├── utils/                    # 工具类
│   │   ├── __init__.py
│   │   ├── logger.py            # 日志系统
│   │   └── startup.py           # 开机启动管理
│   ├── views/                    # 视图层
│   │   ├── __init__.py
│   │   ├── main_window.py       # 主窗口
│   │   ├── components/          # UI组件
│   │   │   ├── __init__.py
│   │   │   ├── data_dialog.py   # 数据管理对话框
│   │   │   ├── history_list.py  # 历史列表组件
│   │   │   ├── prediction_overlay.py  # AI预测浮层
│   │   │   ├── search_bar.py    # 搜索栏组件
│   │   │   ├── settings_dialog.py     # 设置对话框（热键捕获）
│   │   │   └── tray_icon.py     # 托盘图标
│   │   └── styles/              # 样式定义
│   │       ├── __init__.py
│   │       └── main_style.py    # 主题样式
│   └── main.py                  # 程序入口
├── resources/                    # 资源文件
├── requirements.txt              # 依赖列表
└── README.md                     # 项目说明
```

## ⚡ 性能特点

- **启动速度** - 优化后启动时间 < 1秒
- **内存占用** - 空闲时内存 < 50MB
- **数据容量** - 支持 10,000+ 条记录流畅运行
- **响应速度** - 搜索响应 < 100ms

## 🔧 技术栈

- **Python 3.8+** - 编程语言
- **PyQt6** - GUI框架
- **SQLite** - 本地数据存储
- **gzip** - 数据压缩
- **keyboard** - Windows/Linux 键盘监听兼容层
- **pynput** - macOS 全局热键支持
- **langchain** - AI智能预测（可选）
- **pywin32** - Windows系统集成

## ❓ 常见问题

### 热键无法使用
**问题：** 设置的热键没有反应

**解决方案：**
1. 检查是否有其他程序占用了该热键（如截图工具、输入法）
2. 在设置中更换为其他热键组合（推荐 `Ctrl+Alt+O`、`F1`、`F10` 等）
3. 查看日志确认热键是否注册成功
4. macOS 请确认已给终端或宿主应用开启辅助功能权限

### 热键设置捕获失败
**问题：** 点击"设置热键"后按快捷键没有反应

**解决方案：**
1. 确保热键包含修饰键（Ctrl、Alt、Shift、Win 之一）
2. 按 Esc 取消后重新捕获
3. 重启程序后重试

### 程序启动慢
**问题：** 启动时间较长

**解决方案：**
1. 历史记录过多时，程序会延迟加载数据
2. 在设置中减少"最大历史记录数"
3. 开启"启动时最小化到托盘"选项

### macOS 窗口置顶不生效
**问题：** 点击置顶按钮后窗口没有保持在最前面

**解决方案：**
1. 升级到包含 macOS 原生窗口层级修复的版本
2. 该版本会同时设置 Qt 置顶标志和 macOS `NSWindow` 原生层级
3. 如果仍异常，重启应用后再次测试，并查看日志中的 `macOS 原生窗口层级已设置` 提示

### JetBrains 系列 IDE 复制内容未进入历史
**问题：** 在 IntelliJ IDEA、PyCharm 等工具中复制文本后，历史记录里找不到。

**解决方案：**
1. 升级到包含 `v2.0.2` 修复的版本
2. 该版本已改为同时使用 Qt 监听和 Windows 原生剪贴板兜底读取
3. 如果仍异常，优先确认是否存在管理员权限不一致的情况（IDE 与 ClipMaster Pro 建议使用相同权限启动）

### 数据库损坏
**问题：** 程序无法启动或报错

**解决方案：**
1. 备份 `%APPDATA%/ClipMasterPro/data/` 目录
2. 删除或重命名 `clipboard.db` 文件
3. 重启程序会自动创建新数据库

## 📝 更新日志

### v2.0.2 (2026-03-06)
- 🪟 修复 Windows 原生剪贴板读取的 64 位句柄问题
- 🧠 修复 IntelliJ IDEA / PyCharm 复制内容无法进入历史记录的问题
- 🧪 添加独立剪贴板探针脚本，便于排查系统剪贴板格式与变化事件

### v2.0.1 (2026-03-02)
- 🔧 修复热键注册问题，支持双模式注册（RegisterHotKey + keyboard库）
- 🎯 优化热键设置界面，添加捕获按钮模式
- 🐛 修复设置热键时程序崩溃的问题
- 📝 完善 VK 键码映射，支持更多特殊键
- ✨ 添加热键冲突检测和自动降级机制

### v2.0.0 (2026-02-27)
- ✨ 全新UI设计，现代化界面
- ⚡ 性能大幅优化，SQLite数据库存储
- 🖼️ 支持图片、文件等多种格式
- ⭐ 添加收藏功能
- 🔍 增强搜索功能
- 📊 数据压缩和自动清理

### v1.2.0 (2025-06-30)
- 添加数据导入导出功能
- 添加自动清理过期历史记录功能

### v1.1.0 (2025-06-15)
- 添加自动保存功能
- 添加开机自启动支持

### v1.0.0 (2025-06-01)
- 初始版本发布
- 实现基本的剪贴板历史管理功能

## 🛠️ 开发说明

### 本地开发

```bash
# 创建虚拟环境
python -m venv venv

# 激活虚拟环境
venv\Scripts\activate  # Windows
source venv/bin/activate  # macOS/Linux

# 安装开发依赖
pip install -r requirements.txt

# 运行程序
python run.py
# 或
python src/main.py
```

### 打包为可执行文件

```bash
# 安装打包工具
pip install pyinstaller

# 运行打包脚本
python build.py
```

打包后的文件位于 `dist/ClipMasterPro.exe`

## 📄 许可证

本项目采用 MIT 许可证。详情请参阅 [LICENSE](LICENSE) 文件。

## 👨‍💻 关于作者

**Wang Liang**

- 📧 Email: [wl3130187893@gmail.com](mailto:wl3130187893@gmail.com)
- 🌐 GitHub: [@kobewl](https://github.com/kobewl)

---

<div align="center">
  <p>如果您觉得这个项目有用，请给它一个 ⭐️！</p>
  <p>Made with ❤️ by Wang Liang</p>
</div>
