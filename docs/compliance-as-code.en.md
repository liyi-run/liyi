<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

# Compliance as Code — Auditable Intent Chains with 立意

🌍: [简体中文（普通话）](compliance-as-code.zh.md) / English

**Audience:** Tech leads, compliance engineers, QA managers, outsourcing project managers — anyone who needs to demonstrate to auditors or clients that code actually implements requirements. Developers: start with the [README](../README.md).

> ⚠️ **This document is a design-phase narrative draft, not a product commitment.** The 立意 spec format and CLI are under active evolution; expect breaking changes. If you are a compliance engineer or project manager, please disregard this document for now — the current version is intended for project contributors and technical evaluators validating the design direction. It will be updated into a proper scenario guide once the tool stabilizes.

---

## The Problem

When an auditor asks "can you prove this code actually implements the regulatory requirement?" — what do you show them?

Most teams today:

- Requirements live in Jira/Confluence/Notion. Code lives somewhere else. The mapping between them is in someone's head.
- AI agents wrote the code. A reviewer clicked Approve. But nobody can prove the code's *intent* matches the *requirement*.
- Code changed. The requirements doc didn't. Three months later at audit time, nobody knows which changes affected which compliance clause.
- The contractor left. Business logic is scattered across code comments, Slack threads, and stale documents.

This isn't a productivity problem. It's an **accountability** problem.

---

## How 立意 Solves It

立意 is not a "write code faster with AI" tool. It is **intent infrastructure** — ensuring every code item traces back to the intent or requirement it should satisfy, with automated detection when the two fall out of sync.

### Step 1: Anchor requirements to code

Mark regulatory clauses or business requirements directly in code with `@liyi:requirement`. The requirement text lives right next to the code, under the same version control.

```python
# @liyi:requirement aml-transaction-monitoring
# Every transaction must complete AML screening within T+0.
# Transactions exceeding the threshold amount must trigger manual review.
# Screening results must be retained for at least five years.
```

Not a separate document. Not an entry in an external system. The requirement is *right there with the code*.

### Step 2: Bind code to intent

An AI agent infers intent for each code item — what the function *should* do — and records it in a `.liyi.jsonc` sidecar file, declaring which requirements the item relates to:

```jsonc
{
  "item": "screen_transaction",
  "intent": "Perform AML screening on a transaction: validate amount threshold, escalate to manual review queue if exceeded, log screening result with timestamp.",
  "related": { "aml-transaction-monitoring": null },
  "source_span": [42, 78]
}
```

A human reviews the intent and marks `"reviewed": true`. Five lines of intent description instead of fifty lines of implementation — review burden drops by an order of magnitude.

### Step 3: Detect drift

When code or requirements change, `liyi check` detects it automatically:

```plain
screen_transaction: ⚠ REQ CHANGED — requirement "aml-transaction-monitoring" updated
validate_amount:    ⚠ STALE — source changed since spec was written
log_screening:      ✓ reviewed, current
```

- **Requirement changed** → every code item declaring a relationship to that requirement is transitively flagged. The developer must confirm each one: does the code still satisfy the updated requirement?
- **Code changed** → intent is marked stale. The reviewer checks: did the change deviate from declared intent?
- **Both in sync** → green. Pass.

This is what *auditability* means: not a lagging compliance document, but a **live, verifiable chain** — from requirement to intent to code implementation, where every link is change-tracked and drift-detectable.

---

## Why This Matters for Compliance

| Audit need | Traditional approach | 立意 approach |
| ---------- | ------------------- | ------------ |
| Requirement-to-code traceability | Manually maintained mapping tables, easily stale | `@liyi:requirement` + `@liyi:related` form machine-verifiable bidirectional links |
| Change impact analysis | Global search, tribal knowledge | `liyi check` automatically flags all affected code items |
| Review evidence | PR Approve button (proves someone clicked) | `"reviewed": true` + `source_hash` = specific intent confirmed by specific human at specific version |
| Knowledge retention | Depends on original team members' memory | `.liyi.jsonc` persists in the repo, version-controlled alongside code |
| Drift detection | Periodic manual audits (quarterly/annual) | Automatic on every push, runs in CI |

### How this differs from issue trackers

Jira (or Confluence, Linear, etc.) manages *workflow* — who does what, by when, what's the status. 立意 manages *intent* — what this code should do, which requirement it relates to, whether the two are still in sync.

When code changes but the requirement description in Jira doesn't, Jira won't alert you. `liyi check` will.

They're complementary, not competing. Jira is the right tool at the project management layer; 立意 fills the gap Jira doesn't cover at the code-requirement consistency layer.

### Provenance

`.liyi.jsonc` files are checked into version control alongside source code. Review actions (setting `"reviewed": true` in the sidecar) are also part of commit history — VCS automatically records who approved which intent and when. `.liyi.jsonc` files aren't configuration — they're an audit log that evolves with the code.

---

## Scenarios

### Financial compliance

AML rules change. The compliance team updates the threshold and review process description in the requirement text.

```plain
$ liyi check
screen_transaction:   ⚠ REQ CHANGED — requirement "aml-transaction-monitoring" updated
escalate_suspicious:  ⚠ REQ CHANGED — requirement "aml-transaction-monitoring" updated
log_screening:        ✓ current
generate_report:      ✓ current
```

Two functions flagged. The developer opens each, reviews the implementation against the updated requirement, confirms or modifies, and re-marks as reviewed. At audit time, the requirement name traces directly to every item's review history (`git log` provides full provenance).

### Outsourced project delivery

The client changed requirements mid-project. The project manager updates the `@liyi:requirement` text in code.

```plain
$ liyi check
calculate_discount:   ⚠ REQ CHANGED — requirement "discount-rules-v2" updated
apply_coupon:         ⚠ REQ CHANGED — requirement "discount-rules-v2" updated
render_price:         ✓ current
```

Affected code is identified automatically — no global search, no reliance on developer memory. Once each flagged function is addressed and re-reviewed, the `.liyi.jsonc` files become part of the deliverable: proof that "every requirement change was handled."

### Legacy system governance

The contracting team is leaving. Before they go, an AI agent reads the existing code and infers intent for every function, recording it in `.liyi.jsonc`. This is *knowledge capture* — not perfect documentation, but a structured record of "what this code is probably doing," so the internal team taking over doesn't start from zero.

Going forward, `liyi check` runs in CI on every push. Intent drift is continuously detected. Knowledge no longer evaporates with personnel changes.

---

## Technical Properties

| Property | Compliance value |
| -------- | --------------- |
| Zero network dependency | Runs entirely offline, on air-gapped networks — data never leaves the perimeter |
| Zero LLM dependency | Check results are 100% reproducible — same code, same sidecar, same conclusion every time |
| Language-agnostic | Works with legacy systems (Java, PHP) and modern stacks (Rust, Go, Python) in the same repo |
| Lightweight deployment | Single binary, no runtime dependencies, no procurement approval needed |
| CI-friendly | Standard exit codes (0 = pass, 1 = check failure, 2 = internal error), plugs into existing pipelines |
| Progressive adoption | Start with one file, expand to the full repo — no big-bang rollout required |

**Why do compliance checks need a deterministic tool?** If the checker itself calls a probabilistic LLM, today's check passes but tomorrow the same code might fail due to differences in model behavior — this is unacceptable from an audit perspective. `liyi check` uses hash comparison and line tracking; it is mathematically deterministic and 100% reproducible. Intent *inference* is done by AI agents, but the *checker* does not depend on AI.

---

## Getting Started

```bash
# Install
cargo install --path crates/liyi-cli

# Add requirement markers and @liyi:related annotations in code
# Have an AI agent generate .liyi.jsonc intent specs

# Fill hashes and check
liyi check --fix --root .

# Wire into CI
liyi check --root .
```

For detailed usage, see [README.md](../README.md).

立意 is not another SaaS platform that requires a procurement budget. It's a file convention plus a command-line tool — you can try it on one project today, roll it out to the whole team tomorrow, no approval process needed.

---

## Disclaimer

The compliance practices described in this document are based on general software engineering principles and do not constitute legal advice. Consult a qualified legal professional for industry-specific regulatory requirements.

## AIGC Disclaimer

The "compliance as code" positioning originated from market analysis by Kimi K2.5 Thinking (web client). This document was authored by Claude Opus 4.6.
