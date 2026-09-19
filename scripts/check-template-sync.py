#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0 OR MIT
"""Keep the `liyi:template` block identical in AGENTS.md and the design doc.

`liyi init` ships the block between `<!-- liyi:template:start -->` and
`<!-- liyi:template:end -->` from AGENTS.md, so AGENTS.md is the source of
truth. docs/liyi-design.md carries the same block verbatim for human
readers. Because the two copies are byte-identical, checking them is a
plain string comparison — no per-section extraction, no schema-aware JSON
diff — and `--sync` can copy AGENTS.md's block over the design doc's.

Usage:
    python3 scripts/check-template-sync.py [--sync] [ROOT]

Without `--sync` the script exits non-zero and prints a unified diff when
the blocks differ. With `--sync` it rewrites the design doc's block from
AGENTS.md and exits zero. ROOT defaults to the repository root inferred
from this script's location.
"""

from __future__ import annotations

import difflib
import sys
from pathlib import Path

START = "<!-- liyi:template:start -->"
END = "<!-- liyi:template:end -->"


def read(path: Path) -> str:
    return path.read_text(encoding="utf-8")


def extract(text: str, where: str) -> str:
    """The template block without the surrounding markers or newlines."""
    try:
        start = text.index(START) + len(START)
        end = text.index(END, start)
    except ValueError:
        raise SystemExit(f"template-sync: {where} has no {START} / {END} block")
    return text[start:end].strip("\n")


def replace(text: str, block: str) -> str:
    """Rewrite the existing template block in *text* to *block*."""
    start = text.index(START)
    end = text.index(END, start) + len(END)
    return text[:start] + f"{START}\n{block}\n{END}" + text[end:]


def main() -> int:
    args = sys.argv[1:]
    sync = "--sync" in args
    positional = [arg for arg in args if arg != "--sync"]
    if len(positional) > 1:
        raise SystemExit("template-sync: at most one ROOT argument is expected")
    root = Path(positional[0]).resolve() if positional else Path(__file__).resolve().parent.parent

    agents_path = root / "AGENTS.md"
    design_path = root / "docs" / "liyi-design.md"

    source = extract(read(agents_path), "AGENTS.md")
    copy = extract(read(design_path), "docs/liyi-design.md")

    if source == copy:
        print("template-sync: the design doc carries the same template as AGENTS.md")
        return 0

    if sync:
        design_path.write_text(replace(read(design_path), source), encoding="utf-8")
        print("template-sync: copied the AGENTS.md template into docs/liyi-design.md")
        return 0

    print("template-sync: the liyi:template blocks differ (AGENTS.md is the source of truth).\n")
    sys.stdout.writelines(
        difflib.unified_diff(
            source.splitlines(keepends=True),
            copy.splitlines(keepends=True),
            fromfile="AGENTS.md",
            tofile="docs/liyi-design.md",
        )
    )
    print(
        "\nRe-run with `--sync` (or `make sync-template`) to copy the "
        "AGENTS.md block over the design doc's."
    )
    return 1


if __name__ == "__main__":
    sys.exit(main())
