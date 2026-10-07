# #25 实施证据（未候选、未验收）

基线已接受 #32 `765e75941c77537d05e3242ff99f9cfc2b4e8267`；独立 `/tmp/wardrobe-ticket-25` / `codex/ticket-25`。尚需合最新integration及完整联合验收。schema21 / ADR0026。

实际TDD公开Application+临时SQLite：计划创建/重启/CAS修改暂停；真实到期/设备隔离/最新补跑去重/较早错过；durable care worker与取消不发布；limited worker须原业务scope；新计划与修订不得追溯；长停机错过区间；次日目标、不wear；受控真实TCP AI任务等待响应才通知；通知提交前持久claim与unknown不盲重发；portable计划+执行历史及同空间恢复不重复。本目录raw保留各首次失败和修复尝试，文件名含green只表示当时尝试，实际是否通过以结尾exit/test summary为准。

已确认终态：
- plan: 18380 exit101 → 11939 exit0，1/1。
- due: 编译缺可信时钟接口 exit101 → 12020 exit0，2/2。
- worker: 99281 exit101 → 77948 exit0，3/3。72272的Value借用类型错误为编译错误。
- scope: 30862 exit101 → 41479 exit0，4/4。
- pause: 合同实际失败 → 70583 exit0，5/5。
- gap: 合同实际失败 → 29956 exit0，6/6。
- nextday: 17098 exit101（目标日期Null）→48773 exit0，7/7。
- AI: 1788 exit101（未支持计划策略）；63625/23778等修复尝试失败，受控响应items layer1不合法，模型校验后再连接成为unknown；夹具改规范合法layer0后78056 exit0，8/8。期间一次方法签名编译错误单独保留；网络夹具不是真实模型/付费云证明。
- outbox: 缺公开trusted adapter seam的编译red →79671 exit0，9/9。trusted adapter合同仅验证状态机，不冒充OS投递。
- portable: 41079 exit101（未包含计划表）；8448 exit101（时钟夹具仍写已移出的last_evaluated_at，严格validator正确拒绝）；6021/70148实际restore commit失败，诊断SQLITE_CONSTRAINT_FOREIGNKEY787。覆盖时清除旧本机通知队列、保留无FK且空间/成员隔离的本机receipt后49583 exit0，10/10；新安装设备不同、通知默认关闭、历史succeeded及源安装旧备份恢复后不重复仍全部断言。
- GUI：7780首失败为exact getByLabel在select option加载后不再匹配；69075同类计划内容select定位错误。未声称这些失败是产品“缺入口”。改语义combobox locator后61913 exit0，1/1，真实CLI SQLite桥（浏览器）。此时CLI业务源码未包含后续native/CLI-daemon等变更，不能充作最终同源全回归。

所有测试只临时合成安装。未读取用户库/密钥/照片，未启用用户系统自动启动，未调用付费服务。原生代码check、installed macOS/Windows/Linux实际显示/后台/autostart门仍pending；Browser/controlled adapter绿不能替代。

原生官方源码检查：Tauri notification desktop permission_state/request_permission固定Granted；show内部异步忽略notify_rust错误，因此直接使用其notify-rust同步后端记录submitted/unknown。系统是否实际显示与权限是否允许仍不可由返回值证明。
https://raw.githubusercontent.com/tauri-apps/plugins-workspace/v2/plugins/notification/src/desktop.rs
https://v2.tauri.app/plugin/notification/
https://v2.tauri.app/plugin/autostart/

命令均使用实际09独占缓存与native prefix：
```
PKG_CONFIG=/opt/homebrew/opt/pkgconf/bin/pkg-config
PKG_CONFIG_PATH=/tmp/wardrobe-media-native/install/lib/pkgconfig
DYLD_LIBRARY_PATH=/tmp/wardrobe-media-native/install/lib
CARGO_TARGET_DIR=/tmp/wardrobe-implementation/ticket-09/target
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0
cargo test -p wardrobe-core --test schedules
cargo test -p wardrobe-core --features backup-test-diagnostics --test schedules portable_plans -- --nocapture
cargo build -p wardrobe-cli
cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml --locked
npm --prefix apps/desktop run build
WARDROBE_TEST_PORT=5325 npm --prefix apps/desktop run test:e2e -- schedules.spec.ts
```

当前仍在实施：正式input/output schemas、DST缺失/重复时刻合同、时区目标日/手工及未来天气准确边界、child取消/unknown及limited CLI/MCP实际进程、旧19/接受20/new21备份集兼容和领域负例、后台进程重启/多worker、完整成员GUI状态/七宽度/截图审查、同源逐target与native actual三平台证据。不得以首10合同概括完整 #25。

2026-10-07 后续公开合同（仍未候选）：
- DST：58697 首不存在本地时间没有解释记录，exit101；24439 exit0 12/12，秋季重叠选较早UTC一次，春季缺口记录missed/nonexistent_local_time。
- 原生host：73592 owner terminal0，cargo check 2.66s，含可见/最小化与显式后台设置判断、持久化脱敏错误。没有进行真实OS投递/autostart启用。
- 超时通知claim：4983 exit101，submitting永久停留；25702首修复仍exit101（空队列返回未提交事务）；98349 exit0 13/13，超时转unknown且不重发。
- 跨IANA计划日期：46572 exit0 1/1，Kiritimati与Honolulu同日期的本地正午转换不同实际事件日期，前者受当日排除、后者已跨次日不受旧排除。这是新增合同直接通过，未伪造首次red。
仍需当前源CLI/MCP/GUI、剩余授权/恢复队列/未来天气、完整回归与原生三平台实际门，不能按上述局部结果关闭#25。

- 通用AI worker：61918真实limited worker-only调用未拒绝的red；66396 exit0 15/15，关联计划任务必须有recommendation.generate原业务授权（不预复制未验收18通用worker改动）。
- 通知开关batch：42344真实首claim后关闭仍claim下一通知的red；35446 exit0 16/16，同IMMEDIATE内恢复旧unknown并检查当前开关，pending保持；native每次通知前检查foreground/background。此最新native变更还需重新check。
- 当前CLI67312 terminal0，GUI26969 terminal0 1/1，同源至跨IANA合同（后续授权/通知修复发生后不作为最终同源证明）。

- queued restore：89558首attempt用了错误pending.jobs字段为夹具错误；99893正确items公开合同仍失败（任务丢失），64466 exit0 17/17。按本机space/member/revision/device恢复未完成任务，超窗口missed，provider本机提交guard仍禁止重发。
- 95612 exit0 1/1：受控真实TCP请求等待中恢复旧备份，原worker不能跨generation发布；恢复后无queued/pending，执行历史waiting_input。受控HTTP线程合同，不冒充SIGKILL进程或付费服务证明。
- 95020原生check exit0 3.17s，33734前端build exit0，均在queued恢复修复之前，最终同源还需复验。
