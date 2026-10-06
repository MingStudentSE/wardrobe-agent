# #9：真实穿着与实际改穿验收

2026-10-07，macOS arm64。源码分支 `codex/ticket-09-wears`，基于集成 5e3e232；最终提交见 Git 历史。共享 Rust Application、真实临时 SQLite、真实独立 CLI 与 stdin JSON-RPC MCP，以及 Chrome 中真实 React→开发 loopback→CLI→Application；没有替换业务接口或构造模型响应。

## 实际结果

- `cargo test --workspace`：85 passed / 0 failed / 2 ignored。其中 8 个 WearEvent 公共行为测试、14 个独立 CLI/MCP 测试（新增实际穿着测试1个）。忽略项为原有 OS 安全存储与真实抠图模型的显式环境测试，本票未将忽略结果判为已验证模型或安全存储。日志 `rust-tests.log`。
- `npm run test:e2e`：27 passed / 2 skipped。跳过为真实抠图模型、真实本地供应商的显式环境测试，沿用其他票已有验收。新增3个穿着 GUI 流程均实际通过。日志 `gui-tests.log`。
- 最终穿着定向回归：8 Rust tests、3 GUI tests 通过。后者另外验证单品详情中的真实穿着历史。`wear-tests.log`、`wear-gui-final.log`。
- `cargo clippy --workspace --all-targets -- -D warnings` 成功，`cargo fmt --all --check` 与 TypeScript/Vite build 成功。Tauri 独立 Cargo.lock 已同步 chrono-tz 依赖。
- 七个宽度320/375/414/768/1024/1280/1440已实际检查无横向溢出；320/1280截图已查看，继承Atelier色彩、字体、键盘焦点与状态。动作按钮始终可见，无 hover/拖拽依赖。`wears-320.png`、`wears-1280.png`。

## red → green 证据与语义

先运行 `wear.summary`/`wear.record` tracer：unsupported_operation；加入持久记录后同日两事件给出 2次/1天。再运行更正/撤销/历史 tracer：wear.correct unsupported_operation；实现追加修订后，原记录与更正均可读取，撤销后有效计数为零，相同操作键仍返回原收据。

真实资产 tracer 最初解除当前关联后 state从adopted降为preview，违反已存历史；加入wear_asset_refs后，撤销和解除当前关联仍保留已采用manifest。随后多图 tracer证明仅保留display不足，补齐快照全部asset_ids与来源快照引用后通过。资料和模板删除均不覆盖历史 JSON；只更正日期可沿用已删除单品的原快照，改选实际衣物须重新验证同成员、存在、未删除。

GUI tracer最初缺少“穿着”入口；加入真实WearHistory后补记→同日换装→实际改穿→撤销→追加历史→刷新流程通过。第二 tracer最初缺少模板今天穿及单品穿过动作，加入显式确认入口后通过：保存模板不会记穿着。第三通过真实CLI先提交而丢失HTTP响应的边界故障，用户重试复用operation_key，最终只计1次；处理中成员切换禁用，未来日期返回明确错误。

并发两个Application连接对同一根expected_revision=1更正，只有一个成功，另一返回version_conflict；同键不同意图返回idempotency_conflict。成员scope只读授权可以查询不能记录，外来成员ID/事件不能泄漏。同源图片无跨成员身份合并。

跨时区固定fixture：上海2020-01-01 00:30 → 2019-12-31T16:30Z；檀香山2019-12-31 23:30 → 2020-01-01T09:30Z。计数2次/2当地日，不拿UTC创建日期代替当地日。纽约2020-03-08 02:30缺失时刻、2020-11-01 01:30歧义时刻均明确拒绝。仅日期输入保存time_precision=date，排序用当地中午锚点且明确不证明同日先后。

数据库9按实际表初始化，模拟DB9缺表后重开可以记录，不降版本，保留此前服装与模板。旧import DB8兼容测试仍模拟DB8并检查既有版本，最终版本断言更新为公共DATABASE_SCHEMA_VERSION。

## 运行方式与后续界线

```
export PKG_CONFIG=/opt/homebrew/opt/pkgconf/bin/pkg-config
export PKG_CONFIG_PATH=/tmp/wardrobe-media-native/install/lib/pkgconfig
export DYLD_LIBRARY_PATH=/tmp/wardrobe-media-native/install/lib
CARGO_BUILD_JOBS=2 cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
npm --prefix apps/desktop run build
WARDROBE_TEST_PORT=5189 npm --prefix apps/desktop run test:e2e
```

穿着读写操作为wear.record/get/list/history/correct/revoke/summary。CLI外部授权通过--client-stdin，MCP通过本机连接令牌；只在可信本机维护时显式--trusted-local。普通业务无凭据或个人图片。

本票没有交付洗护/洞察/AI反馈或把保存推荐转换为穿着；下游统一消费Application::effective_wears，返回最新非撤销根，按occurred_at/root_event_id稳定排序。没有以本机测试宣称五平台安装、100000事件性能或原生macOS GUI已经完成；这些仍由后续平台/整体验收门槛验证。
