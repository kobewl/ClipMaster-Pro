#!/usr/bin/env python3
"""Check that a release tag matches every version field in the repo.

Fail-closed: if the tag is malformed, or any of package.json, package-lock.json
(top level and packages[""]), src-tauri/tauri.conf.json, src-tauri/Cargo.toml
([package]) or src-tauri/Cargo.lock (clipmaster-pro) is missing, unreadable or
disagrees with the tag, print one GitHub `::error::` line per problem and exit
non-zero.

Usage: check-release-version.py <vX.Y.Z | X.Y.Z>
"""

from pathlib import Path
import argparse
import json
import re

ROOT = Path(__file__).resolve().parent.parent
VERSION = re.compile(r"^\d+\.\d+\.\d+$")
# Cargo 文件用正则解析，不依赖 tomllib（runner 上的 Python 小版本不固定）。
CARGO_PACKAGE_TABLE = re.compile(r"^\[package\][ \t]*$(.*?)(?=^\[|\Z)", re.M | re.S)
CARGO_VERSION_LINE = re.compile(r'^version[ \t]*=[ \t]*"([^"]*)"', re.M)
CARGO_LOCK_ENTRY = re.compile(r'^name = "clipmaster-pro"\nversion = "([^"]*)"', re.M)


class VersionError(Exception):
    pass


def read_text(relative: str) -> str:
    try:
        return (ROOT / relative).read_text(encoding="utf-8")
    except OSError as error:
        raise VersionError(f"无法读取: {error.strerror}") from error


def read_json(relative: str) -> dict:
    try:
        return json.loads(read_text(relative))
    except json.JSONDecodeError as error:
        raise VersionError(f"JSON 解析失败: {error}") from error


def json_field(data: dict, *keys: str) -> str:
    value: object = data
    for key in keys:
        if not isinstance(value, dict) or key not in value:
            raise VersionError(f"缺少字段 {'.'.join(repr(k) for k in keys)}")
        value = value[key]
    if not isinstance(value, str):
        raise VersionError(f"字段 {'.'.join(repr(k) for k in keys)} 不是字符串")
    return value


def cargo_toml_version() -> str:
    table = CARGO_PACKAGE_TABLE.search(read_text("src-tauri/Cargo.toml"))
    if not table:
        raise VersionError("没有 [package] 表")
    line = CARGO_VERSION_LINE.search(table.group(1))
    if not line:
        raise VersionError("[package] 中没有 version")
    return line.group(1)


def cargo_lock_version() -> str:
    entry = CARGO_LOCK_ENTRY.search(read_text("src-tauri/Cargo.lock"))
    if not entry:
        raise VersionError('没有 name = "clipmaster-pro" 的条目')
    return entry.group(1)


CHECKS = [
    ("package.json", lambda: json_field(read_json("package.json"), "version")),
    ("package-lock.json", lambda: json_field(read_json("package-lock.json"), "version")),
    (
        'package-lock.json packages[""]',
        lambda: json_field(read_json("package-lock.json"), "packages", "", "version"),
    ),
    ("src-tauri/tauri.conf.json", lambda: json_field(read_json("src-tauri/tauri.conf.json"), "version")),
    ("src-tauri/Cargo.toml", cargo_toml_version),
    ("src-tauri/Cargo.lock", cargo_lock_version),
]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("tag", help="release tag (vX.Y.Z) or version (X.Y.Z)")
    args = parser.parse_args()

    version = args.tag[1:] if args.tag.startswith("v") else args.tag
    if not VERSION.match(version):
        print(f"::error::release tag must be vMAJOR.MINOR.PATCH, got {args.tag!r}")
        return 1

    failures = 0
    for label, read in CHECKS:
        try:
            found = read()
        except VersionError as error:
            print(f"::error::{label}: {error}")
            failures += 1
            continue
        if found != version:
            print(f"::error::{label}: {found} != {version}")
            failures += 1

    if failures:
        return 1
    print(f"Version {version} consistent across {len(CHECKS)} files")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
