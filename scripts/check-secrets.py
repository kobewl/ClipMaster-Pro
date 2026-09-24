#!/usr/bin/env python3
"""Reject high-confidence credential formats in Git-tracked files.

Only file paths and rule names are printed; matched values never reach CI logs.
"""

from pathlib import Path
import re
import subprocess
import sys


ROOT = Path(__file__).resolve().parent.parent
PATTERNS = {
    "DeepSeek-style API key": re.compile(rb"(?<![A-Za-z0-9])sk-[0-9a-f]{32}(?![A-Za-z0-9])"),
    "GitHub token": re.compile(rb"\b(?:gh[pousr]_[A-Za-z0-9_]{20,}|github_pat_[A-Za-z0-9_]{20,})\b"),
    "AWS access key ID": re.compile(rb"\b(?:AKIA|ASIA)[0-9A-Z]{16}\b"),
    "PEM private key": re.compile(
        rb"-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----\s*[A-Za-z0-9+/=]{32,}"
    ),
}


def main() -> int:
    paths = subprocess.check_output(["git", "ls-files", "-z"], cwd=ROOT).split(b"\0")
    findings = []
    for raw_path in paths:
        if not raw_path:
            continue
        path = ROOT / raw_path.decode("utf-8", errors="surrogateescape")
        if not path.is_file():
            continue
        data = path.read_bytes()
        for name, pattern in PATTERNS.items():
            if pattern.search(data):
                findings.append((path.relative_to(ROOT), name))
    for path, name in findings:
        print(f"credential pattern detected: {path} ({name})", file=sys.stderr)
    if findings:
        return 1
    print(f"Credential scan passed for {len(paths) - 1} tracked files")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
