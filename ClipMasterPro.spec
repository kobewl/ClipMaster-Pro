# -*- mode: python ; coding: utf-8 -*-


a = Analysis(
    ['src/main.py'],
    pathex=['src'],
    binaries=[],
    datas=[('resources', 'resources')],
    hiddenimports=['PyQt6.sip', 'PyQt6.QtCore', 'PyQt6.QtGui', 'PyQt6.QtWidgets', 'objc', 'AppKit', 'Foundation', 'Quartz', 'pynput.keyboard._darwin', 'pynput.mouse._darwin', 'psutil'],
    hookspath=[],
    hooksconfig={},
    runtime_hooks=[],
    excludes=['matplotlib', 'numpy', 'pandas', 'scipy', 'tkinter'],
    noarchive=False,
    optimize=0,
)
pyz = PYZ(a.pure)

exe = EXE(
    pyz,
    a.scripts,
    [],
    exclude_binaries=True,
    name='ClipMasterPro',
    debug=False,
    bootloader_ignore_signals=False,
    strip=False,
    upx=True,
    console=False,
    disable_windowed_traceback=False,
    argv_emulation=False,
    target_arch=None,
    codesign_identity=None,
    entitlements_file=None,
    icon=['/Users/wangliang/Documents/PythonProject/ClipMaster-Pro/build/macos/ClipMasterPro.icns'],
)
coll = COLLECT(
    exe,
    a.binaries,
    a.datas,
    strip=False,
    upx=True,
    upx_exclude=[],
    name='ClipMasterPro',
)
app = BUNDLE(
    coll,
    name='ClipMasterPro.app',
    icon='/Users/wangliang/Documents/PythonProject/ClipMaster-Pro/build/macos/ClipMasterPro.icns',
    bundle_identifier='com.kobewl.clipmasterpro',
)
