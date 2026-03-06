<!-- @liyi:module -->
<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

> ⚠️ **开发中** — 规格格式与 CLI 工具正在积极开发，预期会有破坏性变更。

> English version: [README.md](README.md)

# 《立意》Lìyì — *意在笔先*

**立意**定义了一套规范并提供 CLI 工具，用于在 AI 辅助软件开发中使意图显式化、持久化、可复核。它为每个代码条目配对一条人类可读的意图声明，存储于语言无关的 sidecar 文件（`.liyi.jsonc`）中。CI 检查器（`liyi check`）会检测源码变更超出意图覆盖的情况——捕获过时规格、孤立条目和断裂的需求边——从而使"AI 写了什么"与"人类意图是什么"之间的差距不会悄然扩大。

## 快速开始

```bash
# 安装（从源码）
cargo install --path crates/liyi-cli

# 由智能体生成意图规格（写入 .liyi.jsonc 文件）
# 然后填充哈希：
liyi check --fix --root .

# 在 CI 中运行检查器：
liyi check --root .
```

## 工作原理

1. **由智能体推断意图** — 读取 `AGENTS.md`，为每个代码条目编写 `.liyi.jsonc` sidecar 文件，包含 `source_span` 和自然语言 `intent`。
2. **`liyi check`** — 对源码区间计算哈希，检测过时与偏移，检查复核状态，追踪需求边。零网络、零 LLM、完全确定性。
3. **`liyi reanchor`** — 在有意的代码变更后重新计算区间哈希。不修改意图或复核状态。
4. **由人类复核** — 设置 `"reviewed": true` 或在源码中添加 `@liyi:intent` 以批准。

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

```
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

## 退出码

| 码 | 含义 |
|----|------|
| 0 | 所有规格均为最新，无失败 |
| 1 | 检查失败（过时、未复核、需求已变更） |
| 2 | 内部错误（格式错误的 JSONC、未知 schema 版本） |

## 许可证

`SPDX-License-Identifier: Apache-2.0 OR MIT`
