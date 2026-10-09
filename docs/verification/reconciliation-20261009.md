# issue 对账审计(spec-20261009)

审计基线:main @ 6d3ec37。main 与集成线自 783192e 分叉:main 独有 53 提交(#41–#53 诊断系列);
集成线 codex/beta-integration-continue 独有 347–491 提交,含 11 票全部功能。main 的 crates/wardrobe-core
operations() 仅 27 族,无 sync/schedule/notification/stylist/scenario/purchase;11 票均在 GitHub CLOSED,
关闭 sha 经验证全部为集成线真实祖先(#33 为 codex/beta-integration@5e6540f)。

| issue | 标题 | main 证据 | 分支位置 | 处置 |
| --- | --- | --- | --- | --- |
| #22 | 内置 Stylist | 无 stylist.* 注册 | bic@00cf220;ticket-22 worktree 有未提交骨架(dirty) | 保持关闭+评论;骨架存 patch;随合并票收口 |
| #23 | 场景与旅行计划 | 无 scenarios.* | bic@09937be | 保持关闭+评论 |
| #24 | 购买适配分析 | 无 purchase.* | bic@b1545c1 | 保持关闭+评论 |
| #25 | 定时推荐与系统通知 | 无 schedules.* | bic@22765aa(worktree 已不存在) | 保持关闭+评论 |
| #27 | 本地文件夹同步 | 无 sync.* | bic@8a90059 | 保持关闭+评论 |
| #28 | 同步冲突/事件一致性 | 无 sync/conflicts | bic@846a6b1 | 保持关闭+评论 |
| #29 | S3 同步 | 无 S3 后端 | bic@6301175 | 保持关闭+评论 |
| #30 | WebDAV 同步 | 无 WebDAV 后端 | bic@2b2c194 | 保持关闭+评论 |
| #31 | 同步运维与密钥 | 无 sync.pause/mirror | bic@464a845 | 保持关闭+评论 |
| #33 | 完整外部 Agent 工作流 | 仅基础 CLI/MCP(27 族) | beta-integration@5e6540f(162 契约);cli-contracts@dc6242d 已转干净 | 保持关闭+评论;合并后在 main 复核 capabilities |
| #39 | Web 端 | 完全无 web | bic@f509218(crates/wardrobe-web + scripts/web) | 保持关闭+评论 |

## 统一处置
1. 11 票关闭证据全部属实,不重开;缺口是"集成线 → main 未合并"(bic 领先 491、落后 53,merge-base 783192e)。
2. 开跟踪票执行双向合并;合并后在 main 复核 capabilities 计数与 docs/verification 目录。
3. 遗留现场:ticket-22 worktree 未提交 stylist 骨架已存 patch(external results/spec-20261009/);9729 用户 dirty 按交接文档保持原样。

来源:Explore 代理审计(agent_2b05da67),2026-10-09。
