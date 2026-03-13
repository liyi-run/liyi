#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0 OR MIT
"""Measure the intent-to-code ratio of a liyi-annotated codebase.

Walks the repository looking for source files paired with `.liyi.jsonc`
sidecars, then reports line counts, byte counts, spec counts, and the
resulting review-compression ratio.

Usage:
    python3 scripts/measure-intent-ratio.py [ROOT]

ROOT defaults to the current directory.
"""

from __future__ import annotations

import json
import os
import re
import sys
from pathlib import Path

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

# Source extensions to consider (add more as liyi gains language support).
SOURCE_EXTENSIONS: set[str] = {
    ".rs", ".py", ".go", ".js", ".ts", ".jsx", ".tsx",
    ".c", ".cpp", ".h", ".hpp", ".cs", ".java", ".kt",
    ".rb", ".php", ".swift", ".dart", ".zig", ".sh",
    ".toml", ".json", ".yaml", ".yml",
}

# Directories to skip entirely.
SKIP_DIRS: set[str] = {"target", "node_modules", ".git", "__pycache__", ".mypy_cache"}

# Estimated characters per line for prose (used to convert character counts
# to a human-comparable "lines of prose" figure).
CHARS_PER_LINE = 80


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def strip_jsonc_comments(text: str) -> str:
    """Remove single-line // comments outside of strings (good-enough)."""
    return re.sub(r"//.*$", "", text, flags=re.MULTILINE)


def parse_sidecar(path: str) -> dict | None:
    """Parse a .liyi.jsonc file, tolerating comments."""
    with open(path, "r", encoding="utf-8") as fh:
        raw = fh.read()
    try:
        return json.loads(strip_jsonc_comments(raw))
    except json.JSONDecodeError:
        return None


def is_test_or_fixture(path: str) -> bool:
    parts = path.replace("\\", "/").split("/")
    return "tests" in parts or "fixtures" in parts


# ---------------------------------------------------------------------------
# Collection
# ---------------------------------------------------------------------------


def collect_files(root: Path) -> tuple[list[str], list[str]]:
    """Return (source_files, sidecar_files) under *root*."""
    sources: list[str] = []
    sidecars: list[str] = []

    for dirpath, dirnames, filenames in os.walk(root):
        # Prune skippable directories in-place.
        dirnames[:] = [d for d in dirnames if d not in SKIP_DIRS]

        rel = os.path.relpath(dirpath, root)
        if is_test_or_fixture(rel):
            continue

        for fname in sorted(filenames):
            fp = os.path.join(dirpath, fname)
            if fname.endswith(".liyi.jsonc"):
                sidecars.append(fp)
            elif any(fname.endswith(ext) for ext in SOURCE_EXTENSIONS):
                sources.append(fp)

    sources.sort()
    sidecars.sort()
    return sources, sidecars


# ---------------------------------------------------------------------------
# Measurement
# ---------------------------------------------------------------------------


def file_metrics(path: str) -> dict:
    with open(path, "r", encoding="utf-8") as fh:
        content = fh.read()
    return {"lines": content.count("\n"), "bytes": len(content.encode("utf-8"))}


def measure(root: Path) -> dict:
    sources, sidecars = collect_files(root)

    src_metrics = {fp: file_metrics(fp) for fp in sources}
    sc_metrics = {fp: file_metrics(fp) for fp in sidecars}

    total_src_lines = sum(m["lines"] for m in src_metrics.values())
    total_src_bytes = sum(m["bytes"] for m in src_metrics.values())
    total_sc_lines = sum(m["lines"] for m in sc_metrics.values())
    total_sc_bytes = sum(m["bytes"] for m in sc_metrics.values())

    # Parse sidecars for spec-level analysis.
    total_specs = 0
    prose_count = 0
    prose_chars = 0
    doc_count = 0
    trivial_count = 0
    reviewed_count = 0

    for fp in sidecars:
        doc = parse_sidecar(fp)
        if doc is None:
            continue
        for spec in doc.get("specs", []):
            if "item" not in spec:
                continue
            total_specs += 1
            intent = spec.get("intent", "")
            if intent == "=trivial":
                trivial_count += 1
            elif intent == "=doc":
                doc_count += 1
            else:
                prose_count += 1
                prose_chars += len(intent)
            if spec.get("reviewed", False):
                reviewed_count += 1

    # Pair source ↔ sidecar for per-file table.
    paired: list[dict] = []
    for sc_path in sidecars:
        src_path = sc_path.replace(".liyi.jsonc", "")
        if src_path in src_metrics:
            sl = src_metrics[src_path]["lines"]
            scl = sc_metrics[sc_path]["lines"]
            paired.append({
                "source": os.path.relpath(src_path, root),
                "src_lines": sl,
                "spec_lines": scl,
                "ratio": scl / sl if sl > 0 else float("inf"),
            })

    # Group by module.
    modules: dict[str, dict[str, int]] = {}
    for entry in paired:
        path = entry["source"]
        if "tree_path" in path:
            mod = "tree_path"
        else:
            # Use the crate name.
            parts = path.replace("\\", "/").split("/")
            mod = parts[1] if len(parts) > 1 else "root"
        modules.setdefault(mod, {"src": 0, "spec": 0})
        modules[mod]["src"] += entry["src_lines"]
        modules[mod]["spec"] += entry["spec_lines"]

    intent_prose_lines = prose_chars / CHARS_PER_LINE if prose_chars else 0

    return {
        "source_files": len(sources),
        "sidecar_files": len(sidecars),
        "total_src_lines": total_src_lines,
        "total_src_bytes": total_src_bytes,
        "total_sc_lines": total_sc_lines,
        "total_sc_bytes": total_sc_bytes,
        "total_specs": total_specs,
        "prose_count": prose_count,
        "prose_chars": prose_chars,
        "doc_count": doc_count,
        "trivial_count": trivial_count,
        "reviewed_count": reviewed_count,
        "paired": paired,
        "modules": modules,
        "intent_prose_lines": intent_prose_lines,
    }


# ---------------------------------------------------------------------------
# Report
# ---------------------------------------------------------------------------


def report(m: dict) -> None:
    W = 72
    print("=" * W)
    print("Intent-to-Code Ratio — 立意 (Lìyì)")
    print("=" * W)

    # Per-file table
    print("\n## Per-file breakdown\n")
    hdr = f"{'Source file':<55} {'Src':>5} {'Spec':>5} {'Ratio':>7}"
    print(hdr)
    print("-" * W)
    for p in sorted(m["paired"], key=lambda x: x["source"]):
        print(f"{p['source']:<55} {p['src_lines']:>5} {p['spec_lines']:>5} {p['ratio']:>6.1%}")
    print("-" * W)
    line_ratio = m["total_sc_lines"] / m["total_src_lines"] if m["total_src_lines"] else 0
    print(f"{'TOTAL':<55} {m['total_src_lines']:>5} {m['total_sc_lines']:>5} {line_ratio:>6.1%}")

    # Aggregates
    byte_ratio = m["total_sc_bytes"] / m["total_src_bytes"] if m["total_src_bytes"] else 0
    print(f"\n## Aggregate metrics\n")
    print(f"Source files:              {m['source_files']:>6}")
    print(f"Sidecar files:             {m['sidecar_files']:>6}")
    print(f"Source lines (code):       {m['total_src_lines']:>6}")
    print(f"Sidecar lines (specs):     {m['total_sc_lines']:>6}")
    print(f"Line ratio (spec/code):    {line_ratio:>6.1%}")
    print(f"Source bytes:              {m['total_src_bytes']:>6}")
    print(f"Sidecar bytes:             {m['total_sc_bytes']:>6}")
    print(f"Byte ratio (spec/code):    {byte_ratio:>6.1%}")

    # Spec analysis
    print(f"\n## Spec content analysis\n")
    print(f"Total item specs:          {m['total_specs']:>6}")
    print(f"  Prose intent:            {m['prose_count']:>6}")
    print(f"  =doc sentinels:          {m['doc_count']:>6}")
    print(f"  =trivial sentinels:      {m['trivial_count']:>6}")
    print(f"  Reviewed (approved):     {m['reviewed_count']:>6}")
    print(f"Prose intent total chars:  {m['prose_chars']:>6}")
    if m["prose_count"]:
        print(f"Avg intent length (chars): {m['prose_chars'] / m['prose_count']:>6.0f}")
    if m["total_specs"]:
        print(f"Avg code lines per spec:   {m['total_src_lines'] / m['total_specs']:>6.1f}")

    # Review compression
    ipl = m["intent_prose_lines"]
    print(f"\n## Review compression\n")
    if m["total_src_lines"] and ipl:
        compression_lines = ipl / m["total_src_lines"]
        compression_bytes = m["prose_chars"] / m["total_src_bytes"] if m["total_src_bytes"] else 0
        reduction = m["total_src_lines"] / ipl
        print(f"Pure intent prose (est. lines @ {CHARS_PER_LINE}ch): {ipl:>6.0f}")
        print(f"Compression vs source lines:           {compression_lines:>6.1%}")
        print(f"Compression vs source bytes:           {compression_bytes:>6.1%}")
        print()
        print(f"A reviewer reads ~{ipl:.0f} lines of intent prose")
        print(f"instead of ~{m['total_src_lines']} lines of source code.")
        print(f"That is a {reduction:.1f}x reduction in review surface.")
    else:
        print("(insufficient data)")

    # Per-module
    print(f"\n## Per-module line ratios\n")
    print(f"{'Module':<25} {'Src':>6} {'Spec':>6} {'Ratio':>7}")
    print("-" * 45)
    for mod, vals in sorted(m["modules"].items()):
        r = vals["spec"] / vals["src"] if vals["src"] > 0 else 0
        print(f"{mod:<25} {vals['src']:>6} {vals['spec']:>6} {r:>6.1%}")


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------


def main() -> None:
    root = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(".")
    root = root.resolve()

    if not (root / "AGENTS.md").exists():
        print(f"warning: {root} does not look like a liyi-annotated repo", file=sys.stderr)

    data = measure(root)
    report(data)


if __name__ == "__main__":
    main()
