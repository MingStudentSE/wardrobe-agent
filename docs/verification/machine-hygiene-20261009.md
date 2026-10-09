# 本机卫生清理台账(2026-10-09)

对应 issue #62(Part of #54)。范围:终止孤儿开发进程;工作树逐一核对并保留台账,不删除任何 dirty 或未合入工作树。

## 已终止的孤儿进程

全部满足三条件后终止:PPID=1(父会话已退出)、运行 1 天以上、CPU 0.0%。对应会话已于 10-07/10-08 结束。

| PID | 进程 | 来源 | 运行时长 |
| --- | --- | --- | --- |
| 20749 | webdav_fault_proxy.py (truncate) | worktrees/ticket-30-webdav 测试残留 | 1d22h |
| 20803 | webdav_fault_proxy.py (rate-limit) | 同上 | 1d22h |
| 20853 | webdav_fault_proxy.py (stall) | 同上 | 1d22h |
| 52772 | npm run dev --port 5395 | worktrees/ticket-34-ui 残留 | 1d06h |
| 52795 | vite dev server (127.0.0.1:5395) | 同上 | 1d06h |
| 52806 | esbuild service | 同上 | 1d06h |

验证:六个 PID 全部退出;5395 端口释放(lsof 为空)。保留:/Applications/电子衣橱.app 的 wardrobe-desktop GUI 进程(2026-10-09 修复验证后有意保持运行,供用户人工确认)。

## 工作树台账(git worktree list,2026-10-09)

处置原则(来自 剩余工作与续作交接-20261007.md):含未提交改动或未合入提交的一律保留;本票只记录不删除。

| 工作树 | 分支 | 处置 |
| --- | --- | --- |
| 主检出(备份盘 01-Project) | main | 活跃,保留 |
| external-…/worktrees/integration-continue | codex/beta-integration-continue | 有未合入改动(handoff 警告),保留待 #55 对账 |
| external-…/worktrees/ticket-19-domain / ticket-19-tools | codex/ticket-19-* | #19 已关闭,对账后处置,暂保留 |
| external-…/worktrees/ticket-22-continue | codex/ticket-22-continue | 对账后处置,暂保留 |
| external-…/worktrees/ticket-28-conflicts | codex/ticket-28-conflicts | 对账后处置,暂保留 |
| external-…/worktrees/ticket-33-install | codex/ticket-33-install | 交接文档记录存在,保留 |
| /Users/links/.codex/wardrobe-execution/worktrees/cli-discovery | codex/cli-contracts | 交接文档:含未提交 recommendations.rs/registry_contracts.rs,保留 |
| /Users/links/.codex/worktrees/*(9729/cb19/b184/db9e 等) | 各整合/ detached | cb19 有用户文档改动(交接文档明示保护);9729 为整合工作树;全部保留 |

后续:#55 对账票产出每分支"合入/放弃"结论后,由维护者决定批量清理;本票不做删除动作。
