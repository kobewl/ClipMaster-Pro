#!/usr/bin/env python3
"""Extract one version's section from CHANGELOG.md as release notes.

Fail-closed: if the tag is malformed, or CHANGELOG.md has no (or more than one,
or an empty) section for that version, print a GitHub `::error::` line and exit
non-zero without touching the output file.

Usage: extract-changelog.py <vX.Y.Z | X.Y.Z> --output <file> [--changelog <path>]
"""

from pathlib import Path
import argparse
import re


ROOT = Path(__file__).resolve().parent.parent
VERSION = re.compile(r"^\d+\.\d+\.\d+$")
HEADING = re.compile(r"^## \[(\d+\.\d+\.\d+)\] - (\d{4}-\d{2}-\d{2})[ \t]*$")
SECTION_END = re.compile(r"^## ")
FENCE = re.compile(r"^(```|~~~)")


class ChangelogError(Exception):
    pass


def extract(text: str, version: str) -> list[str]:
    sections: list[list[str]] = []
    current: list[str] | None = None
    in_fence = False
    for line in text.splitlines():
        if FENCE.match(line):
            in_fence = not in_fence
        elif not in_fence:
            heading = HEADING.match(line)
            if heading:
                current = [] if heading.group(1) == version else None
                if current is not None:
                    sections.append(current)
                continue
            if SECTION_END.match(line):
                current = None
                continue
        if current is not None:
            current.append(line)

    if not sections:
        raise ChangelogError(
            f'CHANGELOG.md 中没有 {version} 的章节，请先添加 "## [{version}] - YYYY-MM-DD"'
        )
    if len(sections) > 1:
        raise ChangelogError(f"CHANGELOG.md 中 {version} 的章节出现了 {len(sections)} 次")

    lines = sections[0]
    while lines and not lines[0].strip():
        lines.pop(0)
    while lines and not lines[-1].strip():
        lines.pop()
    if not lines:
        raise ChangelogError(f"CHANGELOG.md 中 {version} 的章节没有内容")
    return lines


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("tag", help="release tag (vX.Y.Z) or version (X.Y.Z)")
    parser.add_argument("--output", required=True, type=Path, help="file to write the notes to")
    parser.add_argument("--changelog", type=Path, default=ROOT / "CHANGELOG.md")
    args = parser.parse_args()

    version = args.tag[1:] if args.tag.startswith("v") else args.tag
    try:
        if not VERSION.match(version):
            raise ChangelogError(f"release tag must be vMAJOR.MINOR.PATCH, got {args.tag!r}")
        try:
            text = args.changelog.read_text(encoding="utf-8")
        except OSError as error:
            raise ChangelogError(f"无法读取 {args.changelog}: {error.strerror}") from error
        lines = extract(text, version)
    except ChangelogError as error:
        print(f"::error::{error}")
        return 1

    args.output.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"Extracted release notes for {version} ({len(lines)} lines)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
