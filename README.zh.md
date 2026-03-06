<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->
<!-- @liyi:module -->

# 《立意》Lìyì — *意在笔先*

🌍: 简体中文（普通话） / [English](README.md)

> ⚠️ **开发中** — 《立意》的协议格式与 CLI 工具正在活跃演变过程中，我们预期会有破坏性变更。
>
> 🐕 **自举完成** — `liyi check` 已能处理自己的代码库了。当 Agent 修改了 Linter 的源码，`liyi check` 立刻报告过时，Agent 自行修复——无需人类额外指令。代码与意图之间的差距自动保持闭合。


《立意》是一套开发规范和配套的 CLI 工具，用于在 AI 辅助的软件开发中，使**意图显式化、持久化、可复核**。它为代码中的每个条目（量、函数的定义等）配对一条人类可读的意图声明，存储于语言无关的 sidecar 文件（`.liyi.jsonc`）中。如果代码实现或原始需求发生变更，导致代码与意图失去同步，配套的 CI 检查器（`liyi check`）便会检测到——过时规格、孤立条目和断裂的需求边——从而使“AI 写了什么”与“要做的是什么”之间的差距不会悄然扩大。

## 快速开始

```bash
# 安装（从源码）
cargo install --path crates/liyi-cli

# 由智能体生成意图规格（写入 .liyi.jsonc 文件）
# 您可自行命令智能体这么做：没有命令调用，智能体就是命令

# 然后填充哈希：
liyi check --fix --root .

# 手工运行，或在 CI 中配置运行检查器：
liyi check --root .
```

## 工作原理

1. **由智能体推断意图** — 当今的智能体会自动读取 `AGENTS.md`，于是便掌握了《立意》设计模式。在正常开发流程中，它们便会自动为每个代码条目维护 `.liyi.jsonc` sidecar 文件，包含 `source_span` 和自然语言 `intent`。如果没有自动维护，也总可以明确告诉它这么干。
2. **`liyi check`** — 为智能体提供的源码区间计算内容哈希，检测内容是否过时、行号是否偏移、是否被复核过，并追踪需求边。零网络访问、零 LLM 依赖、行为完全确定。
3. **`liyi reanchor`** — 在有意的代码变更后重新计算区间哈希。不修改意图或复核状态。
4. **由人类复核** — 在 `.liyi.jsonc` 中设置 `"reviewed": true` 以批准，或在源码中添加 `@liyi:intent` 以明确给出人类版本。

## 渐进式采用

| 级别 | 操作 | 收益 |
|------|------|------|
| 0 | 将 `AGENTS.md` 段落复制到你的仓库 | 由智能体写出 `.liyi.jsonc`，而非一无所有 |
| 1 | 在 CI 中添加 `liyi check` | 每次推送都能检测过时规格 |
| 2 | 复核意图，设置 `reviewed: true` | 确保人类介入语义审查 |
| 3 | 添加 `@liyi:requirement` 标记 | 跨模块关注点被传递追踪 |
| 4 | 在源码中使用 `@liyi:intent` | 意图紧邻代码，经重构不丢失 |
| 5 | 从已复核意图生成对抗测试 | 捕获细微的语义漂移 |

## CLI 参考

```plain
liyi check [OPTIONS] [PATHS]...
    --fix                           自动修正偏移的区间，填充缺失的哈希
    --fail-on-stale <true|false>    对过时规格报错（默认：true）
    --fail-on-unreviewed <true|false>  对未复核规格报错（默认：false）
    --fail-on-req-changed <true|false> 对已变更需求报错（默认：true）
    --root <PATH>                   覆盖仓库根目录

liyi reanchor [FILE]
    --item <NAME>     指定目标条目
    --span <S,E>      覆盖区间（1 起始，闭区间）
    --migrate         执行 schema 版本迁移
```

## 退出状态码

| 状态码 | 含义 |
|----|------|
| 0 | 所有规格均为最新，无失败 |
| 1 | 检查失败（过时、未复核、需求已变更） |
| 2 | 内部错误（格式错误的 JSONC、未知 schema 版本） |

## 许可证

`SPDX-License-Identifier: Apache-2.0 OR MIT`
