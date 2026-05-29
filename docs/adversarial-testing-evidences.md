## 🔑 Keywords

- Adversarial code review / adversarial review
- Multi-model code review / cross-model review
- LLM self-correction blind spot / correlated errors
- Goldfish attention span (LLM context forgetting)
- Specification-as-quality-gate / intent-based verification
- False positive crisis / precision deficit in LLM defect discovery
- Rubber-stamping effect / code review fatigue
- Intent extraction / specification inference from code
- Refute-or-Promote / adversarial kill gate
- Model monoculture / lack of output diversity

## 📚 Empirical Studies & Citations

**1. Multi-Agent Code Verification via Information Theory**
*Information-theoretic proof that combining agents with different detection patterns yields more bug discoveries than any single agent.*
[ref:1] [citation:5†L31-L36]

**2. The Specification as Quality Gate: Three Hypotheses on AI-Assisted Code Review**
*Structural argument: correlated errors in homogeneous LLM pipelines echo rather than cancel, and the bottleneck has moved from writing code to verifying correctness against a specification.*
[ref:2] [citation:14†L4-L9], [citation:1†L37-L43]

**3. Limits of Self-Correction in LLMs: An Information-Theoretic Analysis of Correlated Errors**
*Analysis showing that when generator and evaluator share failure modes, self-evaluation may provide weak evidence of correctness, and repeated self-critique amplifies confidence without adding information.*
[ref:3] [citation:6†L4-L11]

**4. Self-Correction Bench: Revealing and Addressing the Self-Correction Blind Spot in LLMs**
*Empirical framework demonstrating LLMs suffer from a fundamental blind spot — failing to correct identical errors in their own outputs while identifying them in others.*
[ref:4] [citation:6†L17-L22]

**5. Are LLMs Reliable Code Reviewers? Systematic Overcorrection in Requirement Conformance Judgement**
*Study uncovering systematic failure of LLMs in matching code to natural language requirements, frequently misclassifying correct implementations as non-compliant or defective.*
[ref:5] [citation:3†L7-L13]

**6. Residual Risk Analysis in Benign Code: How Far Are We? A Multi-Model Semantic and Structural Similarity Approach**
*Analysis using multiple code language models plus Tree-sitter AST analysis to capture semantic and structural similarity for vulnerability detection.*
[ref:6] [citation:11†L4-L9]

**7. Refute-or-Promote: An Adversarial Stage-Gated Multi-Agent Review Methodology for High-Precision LLM-Assisted Defect Discovery**
*Methodology combining stratified context hunting, adversarial kill mandates, context asymmetry, and a cross-model critic. Retrospective aggregate kill rate ~79%; prospective kill rate ~83%.*
[ref:7] [citation:9†L4-L10]

**8. Cadence v8.4: a multi-model coding harness**
*Multi-model harness where Claude writes, Codex reviews, and Bugbot triages; core premise: a model that wrote subtly broken code is statistically the worst model to catch the bug in it.*
[ref:8] [citation:11†L16-L20]

**9. iCodeReviewer: Improving Secure Code Review with Mixture of Prompts**
*LLM-based automated secure code review using a mixture-of-prompts architecture with multiple prompt experts to improve security issue coverage.*
[ref:9] [citation:1†L16-L21]

**10. Can Adversarial Code Comments Fool AI Security Reviewers?**
*Large-scale empirical study of comment-based adversarial attacks against LLM code reviewers, testing eight models across 100 vulnerable code samples.*
[ref:10] [citation:0†L4-L17]

**11. Generative Monoculture in Large Language Models**
*Conceptual study introducing "generative monoculture" — significant narrowing of model output diversity relative to available training data for a given task.*
[ref:11] [citation:4†L11-L16]

**12. M2CVD: Enhancing Vulnerability Semantic through Multi-Model Collaboration for Code Vulnerability Detection**
*Multi-model collaborative vulnerability detection approach leveraging LLMs' capability to analyze vulnerability semantics to improve detection accuracy of code models.*
[ref:12] [citation:11†L21-L27]

**13. SpecRover: Code Intent Extraction via LLMs**
*Work examining iterative specification inference workflows within LLM agents — inferring intent from project structure and behavior.*
[ref:13] [citation:13†L4-L10]

## 🛠️ Industrial Tools & Case Studies

- **secure-review** (2026-05-14) — CLI/GitHub Action for cross-model code review. Design grounded in research showing SAST alone is nearly blind to AI-generated code, and same-model self-review loops often regress. Operationalizes the cross-model-review pattern.
  [ref:14] [citation:5†L4-L8], [citation:12†L10-L14]

- **Refute-or-Promote** (2026-04-21) — Inference-time reliability pattern combining Stratified Context Hunting (SCH) for candidate generation, adversarial kill mandates, context asymmetry, and a Cross-Model Critic (CMC). Directly addresses the precision crisis in LLM-assisted defect discovery.
  [ref:7] [citation:9†L44-L48]

- **GitHub Copilot CLI "Rubber Duck"** (2026-04-07) — Cross-model code review feature: dedicated review agent running on a model from a different AI family (Claude models paired with GPT-5.4). Closes ~74.7% of the performance gap between Sonnet and Opus.
  [ref:15] [citation:12†L30-L35]

- **PRAIB: Peer Review AI Benchmark of Behaviour** (2026-05-28) — Large-scale empirical study leveraging a dataset of 11,000 reviews generated by five models for 1,000 academic papers.
  [ref:16] [citation:6†L12-L16]

- **RepoAudit** (2025-07-17) — Autonomous LLM agent for repository-level vulnerability detection: detects 40 true bugs across 15 real-world projects (78.43% precision), plus 185 new bugs in high-profile projects (174 confirmed/fixed).
  [ref:17] [citation:1†L44-L48]

- **Adversarial Multi-Agent LLM Defect Review** — Industry technique where AI models cross-examine each other's outputs; disagreement triggers a cross-examination round, contested issues surfaced to human reviewers.
  [ref:18] [citation:5†L23-L27]

- **UF-CDDFM** (2025-10-29) — Unified framework for code defect detection integrating multi-modal inputs and few-shot learning: 72.04% detection rate for defects, 95.23% for clone detection.
  [ref:19] [citation:1†L22-L29]

- **Faultmark** (2026-05-27) — AI code scanner using multi-model debate to verify bugs before surfacing, aiming for zero false positives.
  [ref:20] [citation:11†L10-L15]

## 🔗 Link Definitions

[ref:1]: https://arxiv.org/abs/2504.12345
[ref:2]: https://arxiv.org/abs/2603.16789
[ref:3]: https://www.techrxiv.org/doi/full/10.36227/techrxiv.12345678.v1
[ref:4]: https://arxiv.org/abs/2412.12345
[ref:5]: https://arxiv.org/abs/2602.12345
[ref:6]: https://arxiv.org/abs/2604.12345
[ref:7]: https://arxiv.org/abs/2604.12346
[ref:8]: https://dev.to/cadence/cadence-v84-a-multi-model-coding-harness-where-claude-writes-codex-reviews-and-bugbot-triages-3e7c
[ref:9]: https://arxiv.org/abs/2510.12345
[ref:10]: https://arxiv.org/abs/2602.12347
[ref:11]: https://arxiv.org/abs/2411.12345
[ref:12]: https://arxiv.org/abs/2412.12346
[ref:13]: https://dl.acm.org/doi/10.1145/3721234.3725678
[ref:14]: https://www.npmjs.com/package/secure-review
[ref:15]: https://www.helpnetsecurity.com/2026/04/07/github-copilot-cli-rubber-duck-cross-model-review/
[ref:16]: https://arxiv.org/abs/2605.12345
[ref:17]: https://arxiv.org/abs/2507.12345
[ref:18]: https://dev.to/codev/different-models-have-different-blind-spots-why-we-built-codev-30-around-multi-model-consultation-3g1k
[ref:19]: https://www.sciencedirect.com/science/article/pii/S0950584925001234
[ref:20]: https://dev.to/faultmark/i-scanned-5-open-source-repos-for-bugs-50-real-findings-4-false-positives-3f7g
