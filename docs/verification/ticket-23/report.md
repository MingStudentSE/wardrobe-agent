# Ticket-23（issue #23）场景与旅行计划 · 验收报告

日期：2026-10-07。分支：`codex/ticket-23-scenarios`。实现基线：`codex/beta-integration-continue` @ 22765aa。

## 环境

- 构建全部在备份盘：`CARGO_TARGET_DIR=/Users/links/.codex/wardrobe-execution/cache/ticket23-scenarios`，构建前 source `/Users/links/.codex/wardrobe-execution/native-env.sh`（`set -o pipefail`，真实退出码记录于各日志与下表）。
- GUI 前置：worktree `target/debug/wardrobe-cli` wrapper（`DYLD_LIBRARY_PATH=/Users/links/.codex/wardrobe-execution/native-media/install/lib`，exec 缓存 binary）；`design/studio/assets/*.png` 已在 git 内，无需拷贝。
- 模型测试边界（沿用 #22 的 2026-10-07 用户确认）：模型协议与降级语义用本地脚本化 HTTP oracle（127.0.0.1 有界夹具）覆盖；**真实模型成功需用户自配 OpenAI-compatible 密钥，未在本片实测，如实标注**。

## 自动化结果（全部真实运行，日志在本目录 logs/）

| 验证 | 命令 | 结果 | 退出码 | 日志 |
|---|---|---|---|---|
| TDD RED（实现前） | `cargo test -p wardrobe-core --test scenarios` | 11/11 失败（unsupported_operation，预期失败点） | 101 | logs/tdd-red-core.log |
| 核心域测试（实现后） | `cargo test -p wardrobe-core --test scenarios` | 11/11 通过 | 0 | logs/tdd-green-core.log |
| CLI/MCP 子进程契约 | `cargo test -p wardrobe-cli --test scenario_entrypoints` | 3/3 通过 | 0 | logs/cli-contract.log |
| 全工作区回归 | `cargo test --workspace` | 59 个测试二进制、386 通过、0 失败 | 0 | logs/workspace-full.log |
| clippy（全 workspace、含 tests） | `cargo clippy --workspace --all-targets` | 本片代码 0 告警（存量告警属 #34 已知债，未新增） | 0 | logs/clippy.log |
| GUI 场景 spec | `npx playwright test tests/scenarios.spec.ts`（worktree apps/desktop） | 3/3 通过 | 0 | logs/gui-scenarios.log |
| GUI 邻域回归（main.tsx 加 tab 后） | `npx playwright test tests/outfits.spec.ts tests/stylist.spec.ts` | 4/4 通过 | 0 | logs/gui-neighbors-regression.log |

复现命令（worktree `/Users/links/.codex/wardrobe-execution/worktrees/ticket-23-scenarios`）：

```sh
. /Users/links/.codex/wardrobe-execution/native-env.sh
export CARGO_TARGET_DIR=/Users/links/.codex/wardrobe-execution/cache/ticket23-scenarios
cargo test -p wardrobe-core --test scenarios
cargo test -p wardrobe-cli --test scenario_entrypoints
cargo test --workspace
cd apps/desktop && npm ci && npx playwright test tests/scenarios.spec.ts
```

## 验收标准逐条状态

1. **婚礼/面试和多日旅行基于真实库存完成计划与去重打包清单 — 完成。**
   `scenario.plan`（kind=event，occasion=wedding/interview；kind=travel，2–14 天）基于 active∩洗护可用−排除的真实库存逐天出方案；打包清单按衣物聚合去重（`packing.total` 与条目一致，跨天复用列各天不重复计数）。证据：核心域 `wedding_event_plan_*`、`travel_plan_rotates_days_and_dedupes_packing_list`、CLI `scenario_operations_are_discoverable_and_plan_travel_through_cli`、GUI 测试1。
2. **固定/排除/天气变化/缺库存说明可用，计划可查看修改与保存 — 完成。**
   固定必须落位且归属/可用性服务端校验（`fixed_and_excluded_and_ownership_are_server_validated`：固定+排除冲突、跨成员拒绝、待洗固定拒绝）；排除恒不出现；天气手动/自动可选、失败降级不阻断（`weather_is_optional_reference_*`，未来日期 unavailable 如实标注）；短缺给明确原因（`interview_plan_reports_shortage_*`、CLI 短缺用例、GUI 无解用例）；`scenario.get/list/update/delete` 查看、乐观锁修改（version_conflict 拒绝）与保存均可用（`plans_can_be_viewed_updated_*`、GUI 修改步骤）。
3. **计划不记真实穿着，所有衣物归属/可用性验证，模型失败有明确反馈 — 完成。**
   计划文档 `creates_wear:false`，核心域、CLI、GUI 每条链路都断言 `wear.list` 为空；模型通道收据先行（排队回执前 0 次 POST）、`scenario.worker.run` 单轮真实调用、模型输出经服务端结构校验后采纳（`generation.source=text_model`），失败 3 次尝试后 `rule_fallback` + 失败码 + model_feedback（`model_failure_falls_back_*`），提交结果未知 → waiting_input + `provider_outcome_unknown`，`jobs.retry` 被 `submission_already_sent` 拒绝（`unknown_model_outcome_*`）。真实模型端到端成功需用户密钥另行实测（未实测，如实标注）。
4. **测试外部行为，记录自动化结果；更新领域术语/决策 — 完成。**
   全部断言经公开 Application API / CLI 子进程 / Playwright 驱动真实后端，无占位实现；结果与退出码见表。领域文档更新：`GLOSSARY.md`（场景计划、去重打包清单、跨天轮换降级、计划模型辅助等 7 条）、`docs/adr/0029-scenario-and-travel-plans.md`。

## 已知限制

- 计划暂不进入便携备份/同步（备份 archive 形状是显式版本化契约，本片不扩展）；SQLite 持久化可靠，恢复备份后计划不在其中。
- 真实模型端到端（付费/自配端点）未实测；协议、校验、降级、未知结果语义由本地 HTTP oracle 覆盖。
- 旅行天气只支持出发日参考（现有天气源是当日观测，未来日期如实 unavailable）；逐日预报待天气能力扩展。
- GUI 未提供模型失败后的专用重试按钮（可用新 operation_key 重新发起 AI 计划，或直接使用降级计划）。
