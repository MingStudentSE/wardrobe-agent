Independent Spec-axis review, ticket #32 candidate 1451d47db771f3004feef76889fe1f9ca3e7ecd6 against 1bd7737. Read live issue #32 and parent #1, docs/05 F22/F23, ADR0022, verification README and root handoff; source inspection only, no builds. The first GitHub request timed out; the same issue was subsequently fetched successfully.

0 actionable Spec findings.

Issue #32: “事务一致性备份含成员/单品/搭配/事实，可选择含本地原图并明确范围；无凭据.” backup.rs:11–35 uses an explicit business-table whitelist, including accepted weather/feedback19. :683–744 constructs a new database within the source read transaction rather than copying private pages. Weather cache/errors are cleared; originals are independently opt-in (:747–774). Private provider/client/encryption/jobs/submission-guard tables never enter this export. GUI explicitly labels the package plaintext and its scope (BackupSettings.tsx:24).

Issue #32: “恢复预览、明确覆盖动作、格式/版本/完整性检查与真实恢复成功，失败可回退.” Preview validates format/version, structural/identity/reference/domain constraints and file hashes before mutation (backup.rs:834–908). Feedback validation checks observed snapshots, source proposals, wear ownership, same-root/member earlier predecessor and display references (feedback.rs:76–194). Restore validates an owned copy, creates an originals-inclusive prebackup, then overwrites facts and records its maintenance completion in one IMMEDIATE transaction (backup.rs:949–999). Immutable history and receipts are whitelisted, not reconstructed from current inventory.

Issue #32: “数据目录迁移验证后切换，已有工具读取一致.” data_directory.rs:245–337 copies/validates full installation state and publishes pending then active; resolver rechecks the marker after acquiring the lock (:40–88). Generation and same-device private state are preserved; restore increments generation and clears grants (:981–999). Submitted-call minimum guards survive portable restore locally (jobs.rs:22–79), preventing same-space/member/key blind resubmission without exporting secrets.

Parent: “长任务立即返回任务 ID.” maintenance.rs:273–339 persists immediate receipts; operations are global and member-limited access is rejected (authorization.rs:163–179). Completion receipts and published-move/package reconciliation preserve committed outcomes after cancellation/crash. These supporting guards are aligned scope, not unrelated expansion.

Actual runtime, corruption/rollback, concurrency, three-entry and scale acceptance belongs to the merger. Unaccepted learning/sync extensions and native platform release gates remain explicit future joint coverage; this review does not certify them.
