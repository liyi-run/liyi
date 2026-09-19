#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0 OR MIT
"""Fail if docs/liyi-design.md's reproduced agent content drifts from the
portable template in AGENTS.md.

`liyi init` extracts the block between `<!-- liyi:template:start -->` and
`<!-- liyi:template:end -->` from AGENTS.md and ships it to downstream
repositories, so AGENTS.md is the source of truth for the agent
instruction. The design doc reproduces parts of that block for human
readers, but nothing else keeps the two copies in sync, and they have
silently diverged before (a sentinel added to the template but not the
doc). This check fails when they drift again.

Compared regions:

* the numbered behavioral rules (verbatim),
* the "Key principles of the intent protocol" section (verbatim),
* the `.liyi.jsonc` JSON Schema appendix (compared as parsed JSON, so the
  template's `\\u0040` quine escape and the doc's literal `@` agree).

Intentionally not compared:

* "Resolving rule conflicts" — the doc states it as prose, the template as
  a numbered list; the two are semantically equivalent, not textually
  identical.
* the triage report schema — it has no design-doc counterpart.

Usage:
    python3 scripts/check-template-sync.py [ROOT]

ROOT defaults to the repository root inferred from this script's location.
"""

from __future__ import annotations

import difflib
import json
import re
import sys
from pathlib import Path

TEMPLATE_START = "<!-- liyi:template:start -->"
TEMPLATE_END = "<!-- liyi:template:end -->"
RULES_INTRO = "When writing or modifying code:"
KEY_PRINCIPLES = "### Key principles of the intent protocol"
MINIMAL_INSTRUCTION = "### Minimal instruction (for any AGENTS.md)"
APPENDIX_HEADING = "## Appendix: JSON Schema for `.liyi.jsonc` (v0.1)"


def read(path: Path) -> str:
    return path.read_text(encoding="utf-8")


def template_block(agents: str) -> str:
    """The portable instruction block `liyi init` ships downstream."""
    start = agents.index(TEMPLATE_START) + len(TEMPLATE_START)
    end = agents.index(TEMPLATE_END, start)
    return agents[start:end]


def normalize(text: str) -> str:
    """Strip trailing whitespace and leading/trailing blank lines."""
    lines = [line.rstrip() for line in text.strip().splitlines()]
    return "\n".join(lines).strip()


def rules_region(text: str, stop: str | None = None) -> str:
    """The numbered rules, from the intro line up to *stop* (or the end)."""
    start = text.index(RULES_INTRO) + len(RULES_INTRO)
    end = text.index(stop, start) if stop else len(text)
    return normalize(text[start:end])


def section(text: str, heading: str) -> str:
    """The body of a level-2/3 section, up to the next same-level heading."""
    start = text.index(heading) + len(heading)
    rest = text[start:]
    match = re.search(r"\n#{2,3} ", rest)
    body = rest[: match.start()] if match else rest
    return normalize(body)


def fenced(text: str, info: str) -> list[str]:
    """Bodies of ```<info> fenced code blocks (info must match exactly)."""
    return re.findall(rf"```{info}\n(.*?)\n```", text, re.DOTALL)


def design_rules(design: str) -> str:
    after = design.index(MINIMAL_INSTRUCTION)
    return rules_region(fenced(design[after:], "markdown")[0])


def design_schema(design: str) -> object:
    after = design.index(APPENDIX_HEADING)
    return json.loads(fenced(design[after:], "json")[0])


def agents_schema(template: str) -> object:
    # The first ```json block in the template is the sidecar schema.
    return json.loads(fenced(template, "json")[0])


def diff(name: str, expected: str, actual: str) -> str:
    out = [f"## {name}\n"]
    out.extend(
        difflib.unified_diff(
            expected.splitlines(keepends=True),
            actual.splitlines(keepends=True),
            fromfile="AGENTS.md (source of truth)",
            tofile="docs/liyi-design.md (reproduced copy)",
        )
    )
    return "".join(out)


def main() -> int:
    if len(sys.argv) > 1:
        root = Path(sys.argv[1]).resolve()
    else:
        root = Path(__file__).resolve().parent.parent

    agents = read(root / "AGENTS.md")
    design = read(root / "docs" / "liyi-design.md")

    try:
        template = template_block(agents)
        failures: list[str] = []

        expected_rules = rules_region(template, KEY_PRINCIPLES)
        actual_rules = design_rules(design)
        if expected_rules != actual_rules:
            failures.append(diff("behavioral rules", expected_rules, actual_rules))

        expected_principles = section(template, KEY_PRINCIPLES)
        actual_principles = section(design, KEY_PRINCIPLES)
        if expected_principles != actual_principles:
            failures.append(
                diff("key principles", expected_principles, actual_principles)
            )

        expected_schema = json.dumps(agents_schema(template), indent=2, sort_keys=True)
        actual_schema = json.dumps(design_schema(design), indent=2, sort_keys=True)
        if expected_schema != actual_schema:
            failures.append(diff("sidecar schema", expected_schema, actual_schema))
    except (ValueError, IndexError, json.JSONDecodeError) as exc:
        print(
            "template-sync: could not locate a reproduced region — "
            f"the docs layout changed ({exc}).",
            file=sys.stderr,
        )
        return 1

    if failures:
        print(
            "template-sync: AGENTS.md and docs/liyi-design.md have diverged.\n\n"
            + "\n".join(failures)
            + "\nAGENTS.md is the source of truth; update the design doc's "
            "reproduced copies to match.\n",
            end="",
        )
        return 1

    print("template-sync: design doc reproduces the AGENTS.md template")
    return 0


if __name__ == "__main__":
    sys.exit(main())
