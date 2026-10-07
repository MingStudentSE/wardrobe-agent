# Ticket 18 · 独立验收证据包（independent-20261007）

仅打包 Issue 18 已被独立接受的证据。来源工作树 `/Users/links/.codex/wardrobe-execution/worktrees/merge-18`，
业务提交 **`7a4e3a21a2230ab9c6ddcb1cfee7172abd95f10b`**（`fix(learning): compare reset boundaries by RFC3339 instant`）。
审阅 diff 基线 `b6ee5c8...7a4e3a2`（127 files, +9546/−21）。未修改任何产品/测试/ADR/GLOSSARY 文件；未运行测试、构建、模型或服务；未 commit/push/merge/close。原始作者证据保持原样，不覆盖。

## 来源与出处（provenance/）

- `provenance/git-head-and-diff.txt`：HEAD `7a4e3a2`、标签源 `cd7ef72`（OS 证据与 reset 红绿运行时所在 HEAD，修复当时未提交，随后由 `7a4e3a2` 提交）、审阅基线 `b6ee5c8`、四张截图的 git blob 与 SHA256、`learning.rs` blob。
- `provenance/ticket18-originating-spec.json`：Issue 18 原始验收标准（同步自 results/）。
- 所有日志均为原样复制（含 exit 码文件）；未复制权重、二进制、客户端令牌或用户数据；未做无限量日志复制。

## 各关卡结果（per-gate，全部来自实际命令退出/日志）

| 关卡 | 结果 | 证据 |
| --- | --- | --- |
| reset 时间戳红→绿 | 红 2 failed（exit 101）→ 绿 2 passed（exit 0），无 fixture 变更 | `reset-offset-red-green/red/`、`green/`（`02-learning-reset-offsets.*`） |
| workspace 测试 | **247 passed; 0 failed; 13 ignored**，33 个测试二进制（自实际 `test result:` 行重新点数） | `green/03-workspace-test.*`（exit 0） |
| fmt / clippy | `fmt --check` exit 0（无输出）；`clippy --workspace --all-targets -- -D warnings` exit 0 | `green/05-fmt-check.*`、`green/04-clippy.*` |
| 规模 10k 衣物 / 100k 实际穿着 | 1 passed, exit 0, **73.79s（单次样本，不是 p95）**；enqueue 0ms；显式 65；`source_removed=true`、`reopen=true`、`inferred_stale=true`、`facts_10k100k_preserved=true`；profile 11,726,241 bytes | `green/06-learning-scale-ignored.*`、汇总 `learning-reset-offset-green-summary.json` |
| 重建 CLI/MCP 根复现 | exit **0**；4 个持久 receipt 全部 `queued → succeeded`；重开可查；`lease_token` 仅合成 `null` | `cli-mcp-root-reproduction/learning-entry-final.stdout.json` + `.exit` + `.stderr.log` |
| OS 加密/密文包（pre-reset-change，标签源 `cd7ef72`） | 真实临时系统 Keychain 加密 **6 passed**（exit 0）；双安装密文包 **3 passed**（exit 0）；reproduce exit 0。同一实现未受时间戳改动影响，故以此标签源保留 | `os-evidence-cd7ef72/`（含 `precondition-check.txt`、四个 exit 文件、`learning-entry-os-summary.json`、`learning-entry-os-root-acceptance.json`） |
| 最终 GUI（persistent owner 61973，terminal exit 0） | **58 passed / 3 skipped**（2.0m）。真实已有许可 Qwen 本地端点 `http://127.0.0.1:18087`（Apache-2.0，权重 SHA256 校验一致，synthetic protocol only）与 OS flags 已启用 | `gui-owned-evidence-7a4e3a2/playwright.*`、`local-provider-provenance/` |
| 截图 | 四张最终学习截图（1280/320 × 学习页/来源页），源自同源 GUI | `screenshots/`（hash 见 provenance） |

GUI 3 项 skipped（外部媒体/视觉依赖，不计为通过）：

- `tests/image-editing.spec.ts:15` 公开白衣真实本地模型下载抠图采用并在重启后显示离线缓存
- `tests/local-provider.spec.ts:3` 本地真实 SmolVLM 文本与视觉连接通过 GUI 和共享 CLI
- `tests/recognition.spec.ts:12` 真实本地模型通过 GUI 对公开鞋图返回建议且未审核不写事实

## 红绿精确 oracle（before/after）

Before（红，`red/test.stdout.log` exit 101，2 failed）：

- `wear_created_at_same_instant_plus_fourteen_offset_stays_excluded_after_learning_reset`
  断言 `profile["inferred"] == []` 失败；实际重吸收 `category=top`、`color=blue`（weight 1，wear_ids 1）。事件实际时刻严格早于 cutoff ~53ms，但 `+14:00` 字符串排序在 UTC cutoff 之后。
- `feedback_created_at_same_instant_plus_fourteen_offset_stays_excluded_after_learning_reset`
  断言同上失败；实际重吸收 `category=top|bottom|shoes`、`color=blue`、`color=black`（weight 4）。早于 cutoff ~57ms。
- sanity：重写归档 `preview valid=true` 且 restore 真实成功，事实在场——是产品红，不是 fixture 阻塞。

After（绿，`green/02-learning-reset-offsets.*` exit 0）：同一对测试以同一 fixture 翻绿；`learning.rs` 解析 cutoff 与事件 `created_at` 为 chrono instant，经共享 `predates_reset` 闭包比较，畸形时间戳报 storage error（学习重置时间损坏 / 学习来源记录时间损坏）。修复随后提交为 `7a4e3a2`。

## 两轴独立审查（reviews/，两份分开保留）

- `reviews/ticket18-standards-review.md`：标注轴，基线 `b6ee5c8...7a4e3a2`。
- `reviews/ticket18-spec-review.md`：规格轴，同一基线。缺失/错误需求 **0**；2 条 minor 观察（`learning.edit` 的 `created_at` 实为修订时间；`learning.delete` 永久 suppression 至 reset/rebuild）——两者均已在既有披露中说明。

**关于 standards 审查 "hard" 标签的更正**：审查者把基线既有代码坏味道标为 "hard findings"。根审查将其重新归类为 **8 组判断性观察**（4 组重复代码/原始类型痴迷 + 4 组启发式小项），**记录在案的 standards 违规为 0**；本包原样保留审查文本，仅在此加注，不篡改原文。

## 未完成运行的可追溯记录（不得当作通过或产品失败）

`gui-incomplete-opencode-run-preserved/` 保留先前一次 OpenCode 包装的 GUI 运行日志：playwright 进程消失、无 playwright 退出码（`root-execution-classification.json`：`incomplete_execution_runner_disappeared`，`playwright_exit_missing=true`，`last_completed_test=12`；OpenCode 包装 exit 0 不代表测试完成）。分类为运行器消失，**不是产品绿，也不是产品失败**；随后的 owned 持久 owner 重跑（58 passed / 3 skipped）才是有效 GUI 证据。原始日志一字未改。

## 明确保留的全局关卡（本包不宣称）

- 完整 Beta（三平台安装包、原生窗口验证）未宣称；Beta 仅交付桌面端共享视觉语言。
- 原生 3 OS 验证、CI 账单/门禁未宣称。
- p95 性能未宣称（规模为单次 73.79s 样本）。
- 模型预测质量未宣称（Qwen 仅 synthetic-protocol 回环）。
- Issue 未 push、未关闭、未合并；根审查工件后决定集成。

## 完整性

`MANIFEST.sha256` 列出本文件夹全部文件（不含清单自身）的 SHA256。
