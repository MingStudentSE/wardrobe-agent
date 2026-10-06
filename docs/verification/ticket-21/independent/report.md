# #21 独立合并审查

2026-10-07，macOS。独立工作树 `/tmp/wardrobe-merge-21`，分支 `codex/merge-21`。合并基准 root `4fa138f` + 实现 `88df1ad`，追加 root CI证据 `240c2ea` 后源HEAD `beefa47a8acf42334286ac75ed4715b9baa6c848`。所有运行是此工作树源码构建，不借旧CLI或实现者报告计数。追加CI证据为文档，不改变已测试源码。#13 尚未合入，此报告不宣称其联合终态。

已读取 GitHub #21、规格F16、ADR0016、实现者报告，并独立检查：人物使用权与上传双重同意；专用人物/结果CAS与jobs不变为assets/outbox；成员授权及错误成员拒绝；不可变outfit版本、每件asset哈希/角色/层次；人物第一张与1–9件衣物完整传输、不静默截断；缺图/小图/超限前置失败；provider capability / 开通查询；持久租约、取消及迟到结果拒绝、已提交未知防止重复计费；各方案独立前图、最多4方案、本机保存不改变搭配/衣物/穿着/同步。未发现阻断合并的源码缺陷。

- `cargo test --workspace`：110 passed，0 failed，8 ignored。
- `cargo test -p wardrobe-core --test encryption -- --ignored --test-threads=1`：真实macOS Keychain 6 passed（含原子轮换失败、完整历史epoch恢复）。未把其他ignored模型/凭据测试视为通过。
- `cargo build -p wardrobe-cli` 后，`WARDROBE_ENCRYPTION_OS_TEST=1 WARDROBE_TEST_PORT=5214 npm run test:e2e`：真实Chrome → 当前CLI → 临时SQLite → loopback HTTP，33 passed / 2 skipped（实际抠图模型、SmolVLM环境）。本票协议、全部衣物、两方案比较、保存、重读与七宽度全部执行；Keychain GUI也执行。320截图已人工检查。
- `cargo fmt --all --check`、`cargo clippy --workspace --all-targets -- -D warnings`、frontend build、`git diff --check`通过。

本目录保存独立终态日志。使用独占 `/tmp/wardrobe-ticket-07/target`，`CARGO_INCREMENTAL=0` / jobs2 / native prefix `/tmp/wardrobe-media-native/install`；测试借用symlink已移除，无遗留provider服务或活跃构建，未push/关闭issue。

验收仍有两个真实门槛：已开通付费云 + 明确使用权人物的实际多衣试穿效果；Windows/macOS/Linux原生文件选择器与云任务体验。loopback图只证明协议，不能证明人物一致性或衣物保真；#21仍OPEN。新五平台CI由账户billing/spending限制在runner启动前拦截，见integration-ci-account-block.md，不能归类为源码失败或冒称通过。后续#13合并者完成新联合源的终态回归。
