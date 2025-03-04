# ClipMaster Pro

一个强大而高效的剪贴板管理工具，使用Python构建，通过提供高级剪贴板功能来提升您的工作效率。

## ✨ 主要特性

- 📋 多条目剪贴板历史记录存储
- 🔄 快速访问最近的剪贴板内容
- 🖥️ 用户友好的界面
- 🔒 本地安全存储剪贴板数据
- ⚡ 快速且轻量级
- 🎯 支持快捷键操作
- 💾 自动备份功能

## 🚀 快速开始

### 系统要求

- Python 3.6 或更高版本
- Windows 操作系统

### 安装步骤

1. 克隆仓库：
```bash
git clone https://github.com/kobewl/ClipMaster-Pro.git
cd ClipMaster-Pro
```

2. 创建并激活虚拟环境：
```bash
python -m venv .venv
# Windows系统：
.venv\Scripts\activate
# macOS/Linux系统：
source .venv/bin/activate
```

3. 安装依赖包：
```bash
pip install -r requirements.txt
```

### 使用方法

1. 运行应用：
```bash
python run.py
```

2. 以管理员权限运行（推荐）：
```bash
run_as_admin.bat
```

## 🛠️ 技术细节

- 使用Python构建
- 使用JSON存储剪贴板历史记录
- 实现系统级快捷键功能
- 自动备份系统确保数据安全

## 📁 项目结构

```
clipmaster-pro/
├── src/                    # 源代码目录
├── resources/              # 资源文件
├── clipboard_assistant.py  # 主程序逻辑
├── run.py                 # 程序入口
├── requirements.txt       # Python依赖包
├── start.bat             # 快速启动脚本
└── run_as_admin.bat      # 管理员权限运行脚本
```

## ⚙️ 配置说明

应用程序将剪贴板历史记录存储在 `clipboard_history.json` 中，并在 `clipboard_history.backup.json` 中维护备份。

## 🤝 贡献指南

欢迎提交Pull Request来帮助改进这个项目！

## 📝 开源协议

本项目采用 MIT 协议开源 - 详见 LICENSE 文件

## 🙏 致谢

- 感谢所有为这个项目做出贡献的开发者
- 特别感谢Python社区提供的优秀库和工具

## 📞 支持与帮助

如果您遇到任何问题或有任何疑问，请在GitHub仓库中提交Issue。

---

由 kobewl 用 ❤️ 制作 