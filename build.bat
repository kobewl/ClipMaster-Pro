@echo off
chcp 65001 >nul
echo ============================================
echo ClipMaster Pro - Windows EXE 打包工具
echo ============================================
echo.

:: 检查Python
python --version >nul 2>&1
if errorlevel 1 (
    echo 错误: 未找到Python，请确保Python已安装并添加到PATH
    pause
    exit /b 1
)

:: 安装/升级PyInstaller
echo 正在安装/检查 PyInstaller...
pip install pyinstaller --upgrade -q
if errorlevel 1 (
    echo 错误: 安装PyInstaller失败
    pause
    exit /b 1
)

:: 清理旧构建
echo.
echo 清理旧构建文件...
if exist build rmdir /s /q build 2>nul
if exist dist rmdir /s /q dist 2>nul

:: 使用spec文件打包
echo.
echo 开始打包 ClipMaster Pro...
echo.

pyinstaller ClipMasterPro.spec --noconfirm --clean

if errorlevel 1 (
    echo.
    echo ============================================
    echo 错误: 打包失败！
    echo ============================================
    pause
    exit /b 1
)

:: 复制额外文件
echo.
echo 复制额外文件...
if exist README.md copy README.md dist\ /y >nul
if exist LICENSE copy LICENSE dist\ /y >nul

echo.
echo ============================================
echo 打包成功！
echo ============================================
echo.
echo 输出文件: dist\ClipMasterPro.exe
echo 文件大小: 
for %%I in (dist\ClipMasterPro.exe) do echo   %%~zI 字节
.
echo.
echo 提示:
echo   - 单文件版: dist\ClipMasterPro.exe
echo   - 可以直接复制到任何位置运行
echo   - 首次启动可能需要几秒钟解压
echo.
pause
