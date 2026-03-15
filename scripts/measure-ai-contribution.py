#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0 OR MIT
"""Measure human-only vs AI-assisted contributions from the commit history.

Classifies each commit by inspecting its body for the ``AI-assisted-by:``
trailer.  Reports commit counts, line-change volumes, and breakdowns by
conventional-commit type and scope.

Usage:
    python3 scripts/measure-ai-contribution.py [ROOT]

ROOT defaults to the current directory (must be inside a Git repo).
"""

from __future__ import annotations

import re
import subprocess
import sys
from collections import Counter
from pathlib import Path

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

AI_TRAILER_RE = re.compile(r"^AI-assisted-by:", re.MULTILINE)
# Matches  type(scope): summary  OR  type: summary
CONVENTIONAL_RE = re.compile(r"^(?P<type>[a-z]+)(?:\((?P<scope>[^)]+)\))?:")


def run_git(*args: str, cwd: Path) -> str:
    result = subprocess.run(
        ["git", *args],
        capture_output=True,
        text=True,
        cwd=cwd,
    )
    result.check_returncode()
    return result.stdout


def commit_hashes(cwd: Path) -> list[str]:
    """Return all commit hashes in reverse chronological order."""
    out = run_git("log", "--format=%H", cwd=cwd)
    return [h for h in out.strip().splitlines() if h]


def commit_body(sha: str, cwd: Path) -> str:
    return run_git("log", "-1", "--format=%B", sha, cwd=cwd)


def commit_subject(sha: str, cwd: Path) -> str:
    return run_git("log", "-1", "--format=%s", sha, cwd=cwd).strip()


def commit_date_month(sha: str, cwd: Path) -> str:
    """Return YYYY-MM for the author date."""
    raw = run_git("log", "-1", "--format=%aI", sha, cwd=cwd).strip()
    return raw[:7]


def diffstat(sha: str, cwd: Path) -> tuple[int, int]:
    """Return (insertions, deletions) for a commit."""
    try:
        out = run_git("diff", "--shortstat", f"{sha}^", sha, cwd=cwd)
    except subprocess.CalledProcessError:
        return 0, 0
    ins = dels = 0
    m = re.search(r"(\d+) insertion", out)
    if m:
        ins = int(m.group(1))
    m = re.search(r"(\d+) deletion", out)
    if m:
        dels = int(m.group(1))
    return ins, dels


# ---------------------------------------------------------------------------
# Classification
# ---------------------------------------------------------------------------


def classify_commits(cwd: Path) -> list[dict]:
    """Classify every commit and return a list of records."""
    records: list[dict] = []
    for sha in commit_hashes(cwd):
        body = commit_body(sha, cwd)
        subject = commit_subject(sha, cwd)
        is_ai = bool(AI_TRAILER_RE.search(body))
        m = CONVENTIONAL_RE.match(subject)
        ctype = m.group("type") if m else "other"
        cscope = m.group("scope") if m and m.group("scope") else ""
        ins, dels = diffstat(sha, cwd)
        month = commit_date_month(sha, cwd)
        records.append({
            "sha": sha,
            "subject": subject,
            "ai_assisted": is_ai,
            "type": ctype,
            "scope": cscope,
            "insertions": ins,
            "deletions": dels,
            "month": month,
        })
    return records


# ---------------------------------------------------------------------------
# Report
# ---------------------------------------------------------------------------


def print_report(records: list[dict]) -> None:
    total = len(records)
    ai = [r for r in records if r["ai_assisted"]]
    human = [r for r in records if not r["ai_assisted"]]

    ai_ins = sum(r["insertions"] for r in ai)
    ai_del = sum(r["deletions"] for r in ai)
    hu_ins = sum(r["insertions"] for r in human)
    hu_del = sum(r["deletions"] for r in human)

    pct = lambda n, d: f"{100 * n / d:.1f}%" if d else "N/A"
    sep = "-" * 64

    print("=" * 64)
    print("  Human-only vs AI-assisted contribution report")
    print("=" * 64)
    print()

    # -- Overview ----------------------------------------------------------
    print("## Overview")
    print()
    print(f"  Total commits:      {total}")
    print(f"  AI-assisted:        {len(ai):>5}  ({pct(len(ai), total)})")
    print(f"  Human-only:         {len(human):>5}  ({pct(len(human), total)})")
    print()

    # -- Volume ------------------------------------------------------------
    print("## Line-change volume")
    print()
    print(f"  {'':20s} {'Insertions':>12s} {'Deletions':>12s}")
    print(f"  {sep}")
    print(f"  {'AI-assisted':20s} {'+' + str(ai_ins):>12s} {'-' + str(ai_del):>12s}")
    print(f"  {'Human-only':20s} {'+' + str(hu_ins):>12s} {'-' + str(hu_del):>12s}")
    total_ins = ai_ins + hu_ins
    total_del = ai_del + hu_del
    print(f"  {sep}")
    print(f"  Human share of insertions: {pct(hu_ins, total_ins)}")
    print(f"  Human share of deletions:  {pct(hu_del, total_del)}")
    print()

    # -- By type -----------------------------------------------------------
    print("## Commits by type")
    print()
    ai_types = Counter(r["type"] for r in ai)
    hu_types = Counter(r["type"] for r in human)
    all_types = sorted(set(ai_types) | set(hu_types),
                       key=lambda t: -(ai_types[t] + hu_types[t]))
    print(f"  {'Type':14s} {'Human':>7s} {'AI':>7s} {'Total':>7s}  {'% Human':>8s}")
    print(f"  {sep}")
    for t in all_types:
        h, a = hu_types[t], ai_types[t]
        print(f"  {t:14s} {h:>7d} {a:>7d} {h + a:>7d}  {pct(h, h + a):>8s}")
    print()

    # -- By scope (human only) --------------------------------------------
    print("## Human-only commits by scope")
    print()
    hu_scopes = Counter(r["scope"] for r in human if r["scope"])
    for scope, cnt in hu_scopes.most_common():
        print(f"  {scope:20s} {cnt:>5d}")
    no_scope = sum(1 for r in human if not r["scope"])
    if no_scope:
        print(f"  {'(no scope)':20s} {no_scope:>5d}")
    print()

    # -- Monthly -----------------------------------------------------------
    print("## Monthly activity")
    print()
    months = sorted(set(r["month"] for r in records))
    print(f"  {'Month':10s} {'Human':>7s} {'AI':>7s} {'Total':>7s}")
    print(f"  {sep}")
    for mo in months:
        h = sum(1 for r in human if r["month"] == mo)
        a = sum(1 for r in ai if r["month"] == mo)
        print(f"  {mo:10s} {h:>7d} {a:>7d} {h + a:>7d}")
    print()


# ---------------------------------------------------------------------------
# Entrypoint
# ---------------------------------------------------------------------------


def main() -> None:
    root = Path(sys.argv[1]) if len(sys.argv) > 1 else Path.cwd()
    if not (root / ".git").exists():
        # Walk up to find the repo root.
        for parent in root.parents:
            if (parent / ".git").exists():
                root = parent
                break
        else:
            print("Error: not inside a Git repository.", file=sys.stderr)
            sys.exit(1)

    records = classify_commits(root)
    if not records:
        print("No commits found.", file=sys.stderr)
        sys.exit(1)
    print_report(records)


if __name__ == "__main__":
    main()
