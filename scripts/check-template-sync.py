#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0 OR MIT
"""Keep AGENTS.md's portable template in sync with the design doc's block.

`liyi init` ships the `START liyi agent instructions` block from `AGENTS.md`
unchanged, so that block keeps its section heading and `liyi init` needs no
special-casing. `docs/liyi-design.md` carries the same instruction body
*without* the heading, so the design doc's own section heading and outline
stay in control. This check compares the two, accounting for the heading:
AGENTS.md's block must equal the design doc's block prefixed with the
heading.

`--sync` materializes AGENTS.md from the design doc's block, adding the
heading, so the design doc is the source of truth.

Usage:
    python3 scripts/check-template-sync.py [--sync] [ROOT]

Without `--sync` the script exits non-zero and prints a unified diff when
the copies differ. With `--sync` it rewrites AGENTS.md's block from the
design doc's and exits zero. ROOT defaults to the repository root inferred
from this script's location.
"""

from __future__ import annotations

import difflib
import sys
from pathlib import Path

START = "<!-- START liyi agent instructions rev. 1 -->\n<!-- DON'T EDIT THIS BLOCK. REFRESH WITH `liyi migrate` FROM A NEWER LIYI. -->"
END = "<!-- END liyi agent instructions -->"
HEADING = "## The 立意 (Intent Specs) design pattern for agents"
# @liyi:related agents-md-instructions-section-naming


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

    agents = read(agents_path)
    body = extract(read(design_path), "docs/liyi-design.md")
    expected = f"{HEADING}\n\n{body}"
    actual = extract(agents, "AGENTS.md")

    if actual == expected:
        print("template-sync: AGENTS.md matches the design doc's template block")
        return 0

    if sync:
        agents_path.write_text(replace(agents, expected), encoding="utf-8")
        print(
            "template-sync: wrote the design doc's block into AGENTS.md "
            "(section heading added)"
        )
        return 0

    print(
        "template-sync: AGENTS.md does not match the design doc's template "
        "block (the design doc is the source of truth).\n"
    )
    sys.stdout.writelines(
        difflib.unified_diff(
            expected.splitlines(keepends=True),
            actual.splitlines(keepends=True),
            fromfile="docs/liyi-design.md (expected, heading added)",
            tofile="AGENTS.md",
        )
    )
    print(
        "\nRe-run with `--sync` (or `make sync-template`) to copy the design "
        "doc's block into AGENTS.md."
    )
    return 1


if __name__ == "__main__":
    sys.exit(main())
