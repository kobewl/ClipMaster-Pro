#!/usr/bin/env python3
"""
ClipMaster Pro - macOS app bundle build script.
Builds a standalone .app via PyInstaller.
"""

from __future__ import annotations

import shutil
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parent
BUILD_DIR = ROOT / "build" / "macos"
DIST_DIR = ROOT / "dist"
ICONSET_DIR = BUILD_DIR / "ClipMasterPro.iconset"
ICNS_PATH = BUILD_DIR / "ClipMasterPro.icns"
LOGO_PATH = ROOT / "resources" / "logo.png"


def run(cmd: list[str]) -> None:
    print(">", " ".join(str(part) for part in cmd))
    subprocess.run(cmd, check=True, cwd=ROOT)


def ensure_pyinstaller() -> None:
    run([sys.executable, "-m", "pip", "install", "pyinstaller"])


def clean() -> None:
    for path in (BUILD_DIR, DIST_DIR / "ClipMasterPro.app"):
        if path.exists():
            shutil.rmtree(path, ignore_errors=True)
    BUILD_DIR.mkdir(parents=True, exist_ok=True)


def make_icns() -> Path | None:
    if not LOGO_PATH.exists():
        return None
    if shutil.which("sips") is None or shutil.which("iconutil") is None:
        return None

    if ICONSET_DIR.exists():
        shutil.rmtree(ICONSET_DIR)
    ICONSET_DIR.mkdir(parents=True, exist_ok=True)

    icon_sizes = [
        (16, "icon_16x16.png"),
        (32, "icon_16x16@2x.png"),
        (32, "icon_32x32.png"),
        (64, "icon_32x32@2x.png"),
        (128, "icon_128x128.png"),
        (256, "icon_128x128@2x.png"),
        (256, "icon_256x256.png"),
        (512, "icon_256x256@2x.png"),
        (512, "icon_512x512.png"),
        (1024, "icon_512x512@2x.png"),
    ]

    for size, filename in icon_sizes:
        run(
            [
                "sips",
                "-z",
                str(size),
                str(size),
                str(LOGO_PATH),
                "--out",
                str(ICONSET_DIR / filename),
            ]
        )

    run(["iconutil", "-c", "icns", str(ICONSET_DIR), "-o", str(ICNS_PATH)])
    return ICNS_PATH


def build_app(icon_path: Path | None) -> None:
    pyinstaller_args = [
        sys.executable,
        "-m",
        "PyInstaller",
        "--noconfirm",
        "--clean",
        "--windowed",
        "--name",
        "ClipMasterPro",
        "--paths",
        "src",
        "--osx-bundle-identifier",
        "com.kobewl.clipmasterpro",
        "--add-data",
        "resources:resources",
        "--hidden-import",
        "PyQt6.sip",
        "--hidden-import",
        "PyQt6.QtCore",
        "--hidden-import",
        "PyQt6.QtGui",
        "--hidden-import",
        "PyQt6.QtWidgets",
        "--hidden-import",
        "objc",
        "--hidden-import",
        "AppKit",
        "--hidden-import",
        "Foundation",
        "--hidden-import",
        "Quartz",
        "--hidden-import",
        "pynput.keyboard._darwin",
        "--hidden-import",
        "pynput.mouse._darwin",
        "--hidden-import",
        "psutil",
        "--exclude-module",
        "matplotlib",
        "--exclude-module",
        "numpy",
        "--exclude-module",
        "pandas",
        "--exclude-module",
        "scipy",
        "--exclude-module",
        "tkinter",
        "src/main.py",
    ]

    if icon_path is not None:
        pyinstaller_args[-1:-1] = ["--icon", str(icon_path)]

    run(pyinstaller_args)


def main() -> None:
    print("ClipMaster Pro - macOS packaging")
    ensure_pyinstaller()
    clean()
    icon_path = make_icns()
    build_app(icon_path)
    print(f"\nBuilt app: {DIST_DIR / 'ClipMasterPro.app'}")


if __name__ == "__main__":
    main()
