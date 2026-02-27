#!/usr/bin/env python3
"""
ClipMaster Pro - Windows EXE 打包脚本
使用 PyInstaller 打包成独立可执行文件
"""

import os
import sys
import shutil
import subprocess
from pathlib import Path


def clean_build_dirs():
    """清理构建目录"""
    dirs_to_clean = ['build', 'dist', '__pycache__', '*.spec']
    for pattern in dirs_to_clean:
        for path in Path('.').glob(pattern):
            if path.is_dir():
                shutil.rmtree(path, ignore_errors=True)
                print(f"已清理目录: {path}")
            elif path.is_file():
                path.unlink()
                print(f"已清理文件: {path}")


def build_exe():
    """构建EXE文件"""
    
    # 确保依赖已安装
    print("检查依赖...")
    subprocess.run([sys.executable, '-m', 'pip', 'install', 'pyinstaller', '-q'], check=True)
    
    # PyInstaller 参数
    pyinstaller_args = [
        'pyinstaller',
        '--name=ClipMasterPro',
        '--windowed',  # 不显示控制台窗口
        '--onefile',   # 打包成单个文件
        '--noconfirm', # 不确认覆盖
        '--clean',     # 清理临时文件
        
        # 添加数据文件
        '--add-data=src;src',
        '--add-data=resources;resources',
        
        # 隐藏导入（PyQt6需要）
        '--hidden-import=PyQt6.sip',
        '--hidden-import=PyQt6.QtCore',
        '--hidden-import=PyQt6.QtGui',
        '--hidden-import=PyQt6.QtWidgets',
        '--hidden-import=win32com',
        '--hidden-import=win32com.client',
        
        # 排除不必要的模块以减小体积
        '--exclude-module=matplotlib',
        '--exclude-module=numpy',
        '--exclude-module=pandas',
        '--exclude-module=scipy',
        '--exclude-module=tkinter',
        '--exclude-module=unittest',
        '--exclude-module=pytest',
        '--exclude-module=pydoc',
        '--exclude-module=email',
        '--exclude-module=http',
        '--exclude-module=xml',
        '--exclude-module=xmlrpc',
        '--exclude-module=html',
        '--exclude-module=lib2to3',
        '--exclude-module=multiprocessing.popen_spawn_win32',
        '--exclude-module=multiprocessing.popen_forkserver',
        '--exclude-module=multiprocessing.popen_spawn_posix',
        '--exclude-module=distutils',
        '--exclude-module=setuptools',
        '--exclude-module=pip',
        '--exclude-module=pkg_resources',
        '--exclude-module=wheel',
        
        # 优化
        '--strip',  # 去除符号表
        
        # 主程序入口
        'src/main.py'
    ]
    
    # 如果有图标文件，添加图标
    icon_path = Path('resources/icon.ico')
    if icon_path.exists():
        pyinstaller_args.insert(1, f'--icon={icon_path}')
        print(f"使用图标: {icon_path}")
    
    print("\n开始打包...")
    print(f"命令: {' '.join(pyinstaller_args)}\n")
    
    try:
        result = subprocess.run(pyinstaller_args, check=True, capture_output=False)
        print("\n✅ 打包成功!")
        return True
    except subprocess.CalledProcessError as e:
        print(f"\n❌ 打包失败: {e}")
        return False


def create_portable_version():
    """创建便携版（文件夹形式）"""
    print("\n创建便携版...")
    
    pyinstaller_args = [
        'pyinstaller',
        '--name=ClipMasterPro-Portable',
        '--windowed',
        '--onedir',    # 打包成文件夹
        '--noconfirm',
        '--clean',
        
        '--add-data=src;src',
        '--add-data=resources;resources',
        
        '--hidden-import=PyQt6.sip',
        '--hidden-import=PyQt6.QtCore',
        '--hidden-import=PyQt6.QtGui',
        '--hidden-import=PyQt6.QtWidgets',
        '--hidden-import=win32com',
        '--hidden-import=win32com.client',
        
        '--exclude-module=matplotlib',
        '--exclude-module=numpy',
        '--exclude-module=pandas',
        '--exclude-module=scipy',
        '--exclude-module=tkinter',
        
        'src/main.py'
    ]
    
    icon_path = Path('resources/icon.ico')
    if icon_path.exists():
        pyinstaller_args.insert(1, f'--icon={icon_path}')
    
    try:
        subprocess.run(pyinstaller_args, check=True, capture_output=True)
        print("✅ 便携版创建成功!")
        return True
    except subprocess.CalledProcessError as e:
        print(f"❌ 便携版创建失败: {e}")
        return False


def copy_additional_files():
    """复制额外文件到输出目录"""
    dist_dir = Path('dist')
    
    # 复制README
    if Path('README.md').exists():
        shutil.copy('README.md', dist_dir / 'README.md')
        print("已复制 README.md")
    
    # 复制LICENSE
    if Path('LICENSE').exists():
        shutil.copy('LICENSE', dist_dir / 'LICENSE')
        print("已复制 LICENSE")
    
    print(f"\n输出目录: {dist_dir.absolute()}")


def main():
    """主函数"""
    print("=" * 60)
    print("ClipMaster Pro - Windows EXE 打包工具")
    print("=" * 60)
    
    # 清理旧构建
    print("\n清理旧构建文件...")
    clean_build_dirs()
    
    # 构建单文件版
    if build_exe():
        copy_additional_files()
        
        # 询问是否创建便携版
        print("\n是否创建便携版(文件夹形式)? 启动更快但文件较多")
        response = input("输入 y 创建, 其他键跳过: ").strip().lower()
        
        if response == 'y':
            create_portable_version()
            copy_additional_files()
        
        print("\n" + "=" * 60)
        print("打包完成!")
        print(f"输出位置: {Path('dist').absolute()}")
        print("=" * 60)
    else:
        print("\n打包过程中出现错误，请检查输出信息。")
        sys.exit(1)


if __name__ == '__main__':
    main()
