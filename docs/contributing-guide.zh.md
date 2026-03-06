<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

# AI 智能体贡献指南

本文档是面向 AI 智能体的维护者首选贡献指南的中文版本。英文版请参阅 [contributing-guide.en.md](contributing-guide.en.md)。

## 核心要求

- 保持变更最小化、范围明确。
- 每个提交只做一件事（不在同一提交中混入无关修改）。
- 避免重新格式化无关文件。
- 提交前审查 diff，排除无关变更。
- 在架构变更时同步更新 `AGENTS.md`。

## AIGC 政策

本项目执行严格的人工智能生成内容（AIGC）政策。**所有 AI 智能体在贡献之前必须阅读并遵守完整政策。**

- English: [aigc-policy.en.md](aigc-policy.en.md)
- 中文: [aigc-policy.zh.md](aigc-policy.zh.md)

要点（以完整政策为准）：

- **提交拆分**：每个提交的内容要么完全由人类撰写，要么完全由 AI 撰写，不得混合。
- **身份披露**：AI 智能体必须在每个 AIGC 提交中添加 `AI-assisted-by` trailer（如 `AI-assisted-by: Claude Opus 4.6 (GitHub Copilot)`）。
- **原始提示词**：在提交说明正文中（trailer 之前）记录用户的原始提示词。
- **人类复核**：所有提交必须经人类复核，由人类添加 `Signed-off-by`（DCO）。AI 智能体**禁止**代替用户添加此标签。
- **禁止敏感信息**：提交中不得包含密钥、凭据或个人隐私数据。

## 项目概况

- **定位**：立意（Lìyì）定义了一套规范并提供相应工具，用于在 AI 辅助软件开发中使意图显式化、持久化、可复核。本项目*不是*智能体技能集——它产出的是实践规范、检查器、规格格式以及可供其他项目采用的智能体技能模板。
- **语言**：以 Markdown 为主（设计文档、模板），随项目成熟预计将包含 Rust 等语言的实现代码。
- **许可证**：`Apache-2.0 OR MIT`

仓库结构：

- `docs/`：设计文档、AIGC 政策、贡献指南及其他项目级文档。
- `AGENTS.md`：本仓库的智能体指令。
- `README.md`：项目介绍。

随着项目发展，预计将增加用于实现（检查器、CLI）和交付智能体技能模板的顶层目录。

## 内容工作流

### 设计文档

- 设计文档（`docs/liyi-design.md`）是权威规格说明，对其的变更应深思熟虑、动机充分。
- 提议设计变更时，请在提交说明正文中阐明理由。

### 智能体技能模板

- 智能体技能模板是本项目的交付物之一——一组可供下游仓库采用和定制的文件（AGENTS.md、贡献指南、AIGC 政策）。
- 编辑模板内容时，请同时考虑本项目自身使用和下游采用体验。模板文件应具有明确的通用性。

### 文档

- 项目级文档位于 `docs/`。
- AIGC 政策文档（`docs/aigc-policy.*.md`）具有规范性——不得在其他文档中削弱或与之矛盾。

### 双语要求

- 必须为所有文档提供**中英文双语版本**，以最大化传播范围。
- 创建或修改文档时，须在同一提交或紧随其后的提交中同步另一语言版本的变更。
- 代码标识符和提交说明须以英文书写。

## 代码风格与规范

- 为 Markdown 文件使用 ATX 风格标题（`#`、`##` 等）。
- 在标题后、代码块前使用空行。
- 行宽应保持在合理长度（建议在 80–100 字符左右折行）。
- 应在文档文件顶部添加 SPDX 许可证头（`<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->`）。

### 自引用转义约定（Quine-escape）

检查器通过纯子串匹配在源文件中扫描 `@liyi:*` 标记——不具备语言感知能力。因此，包含标记文本字面量的字符串常量、格式化字符串和测试数据会被误识别为真实标记（即程序读取自身源码的经典"自引用"问题）。

为防止自触发，请在任何拼写出标记的字符串字面量中**转义 `@` 字符**：

| 语言 | 转义方式 | 示例 |
|---|---|---|
| Rust | `\x40` | `"\x40liyi:ignore"` |
| JSON | `\u0040` | `"\u0040liyi:requirement"` |

实际的标记注释（如 `// @liyi:intent =doc`）必须保留字面 `@`——它们是真正的标记。

此不变量以 `@liyi:requirement(quine-escape)` 形式记录在 `src/markers.rs` 中。

### 中文写作风格

撰写中文内容时，应避免使用**主题—评述（topic-comment）句式**，改用**主谓结构**并搭配适当的介词或助词，以便不同背景的读者更容易理解。

具体要求：

- **避免将主题直接充当主语而省略介词**，应使用"对于""为""在"等介词明确语义关系。
- **无生命主语不要省略被动标记**（"被""由"等），或者改写为显式的主动句并补全隐含的宾语。

示例：

| ❌ 主题—评述（避免） | ✅ 主谓结构（推荐） |
|---|---|
| 设计文档描述了规范 | 在设计文档中描述了规范 |
| Markdown 文件使用 ATX 风格标题 | 为 Markdown 文件使用 ATX 风格标题 |
| 所有文件均须提供双语言版本 | 必须为所有文件提供双语言版本 |

## 提交说明风格

遵循 Conventional Commits 规范：

```plain
<type>(<scope>): <summary>
```

准则：

- 使用祈使语气、现在时态的简要描述（末尾不加句号）。
- 将简要描述控制在约 50–72 个字符。
- 每个提交只做一件事——不要合并无关变更。
- 必要时在正文中说明动机或关键变更。
- 在正文与简要描述之间用空行分隔；正文的每行应在约 72 字符处折行。

类型（type）：

- `feat`：新功能或新能力
- `fix`：Bug 修复或纠正
- `docs`：文档变更（设计文档、指南、政策）
- `refactor`：不改变行为的重构
- `build`：构建系统或依赖变更
- `ci`：CI/CD 配置变更

**不要**使用 `chore`——请根据情况使用 `build` 或 `ci`。

范围（scope）：

- `design`：设计文档（`docs/liyi-design.md`）
- `linter`：检查器实现
- `template`：智能体技能模板交付物
- `docs`：通用文档
- `policy`：AIGC 政策
- `meta`：仓库元数据（README、AGENTS.md、许可证）

### AIGC 提交要求

根据 [AIGC 政策](aigc-policy.zh.md)，AI 智能体必须：

1. 使用 `AI-assisted-by` trailer 披露身份。
2. 在提交说明正文中记录原始提示词。
3. **禁止**代替用户添加 `Signed-off-by`。

示例：

```plain
feat(linter): implement source hash comparison

Add source hash computation and comparison logic for detecting
stale intent specs.

Original prompt:

> Implement the source_hash staleness check described in the
> design document section on linter behavior.

AI-assisted-by: Claude Opus 4.6 (GitHub Copilot)
Signed-off-by: Contributor Name <contributor@example.com>
```

## 验证清单

提交前请检查：

- ✅ Markdown 文件格式正确（无断链、标题层级合理）。
- ✅ 新文档文件包含 SPDX 许可证头。
- ✅ 中英文双语版本已同步创建或更新。
- ✅ 提交说明符合 Conventional Commits 规范及 AIGC 政策要求。
- ✅ 不包含敏感信息。
- ✅ 变更范围限于单一逻辑单元。
