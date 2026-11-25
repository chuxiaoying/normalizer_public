#!/usr/bin/env python3
"""Check relative Markdown links tracked by this repository."""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path


LINK = re.compile(r"\[[^\]]+\]\(([^)]+)\)")


def tracked_markdown(root: Path) -> list[Path]:
    output = subprocess.check_output(
        ["git", "-C", str(root), "ls-files", "-z", "*.md"], text=True
    )
    return [root / path for path in output.split("\0") if path]


def main() -> int:
    root = Path(__file__).resolve().parents[3]
    missing: list[tuple[Path, str]] = []
    files = tracked_markdown(root)
    for markdown in files:
        text = markdown.read_text(encoding="utf-8")
        for match in LINK.finditer(text):
            target = match.group(1).split("#", 1)[0].strip()
            if not target or "://" in target or target.startswith("mailto:"):
                continue
            if not (markdown.parent / target).resolve().exists():
                missing.append((markdown.relative_to(root), target))

    if missing:
        for markdown, target in missing:
            print(f"{markdown}: {target}", file=sys.stderr)
        print(f"missing Markdown links: {len(missing)}", file=sys.stderr)
        return 1

    print(f"checked Markdown links in {len(files)} tracked files: ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
