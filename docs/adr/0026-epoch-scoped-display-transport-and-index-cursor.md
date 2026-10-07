# ADR 0026：纪元隔离的商品图传输与同步索引游标单调性

状态：已实施；ticket-27 同步分支，含实际公开 API 红/绿回归（media epoch、legacy 扁平兼容、双连接游标并发）。

## 商品图传输格式兼容性

自本票起，已采纳（adopted + workspace 范围）商品展示图的 blob commit 按工作区纪元写入 `{space}/epochs/{epoch}/blobs`；纪元 1 保持旧扁平 `{space}/blobs`，不迁移、不重写既有密文对象。信封格式不变：`{"protocol_version":1,"identity":{...},"envelope":{...}}`，其中 identity 为明文、envelope 由当前纪元主密钥封印。解码器校验保持全部不变：信封 `identity.epoch` 必须等于设备当前纪元，否则拒绝（sync_state_required）；发布者清单签名、设备名单与已撤销设备拒绝照旧。

旧版本传输会把当前纪元的 commit 写进扁平 `{space}/blobs`。发现阶段在当前纪元目录之外以扁平目录作为兼容回退：仅按明文 `identity.epoch` 预过滤，其它纪元或不可解析对象直接跳过、永不解密；每个被接受的 commit 仍走完整解密与验证。chunk 对象的物理位置由 commit 索引内的绝对对象键决定，读取方不假设其目录，因此新旧传输产物可在同一空间共存。

纪元推进后，发布方按当前纪元键派生对象名；新纪元目录为空即自然重新发布当前衣物关联及历史穿搭、养护事实仍引用的已采纳本地商品展示图，换纪后全新配对设备依赖该重发与扁平兼容回退取图，原图与个人字节永不同步（received asset 的 original 恒为空）。

## 同步索引游标单调性

`sync_index_read` 行的持久化改为单条原子 upsert：`observed_head` 以 `max` 只增不减；`next_page` 与 `previous_digest` 仅在同一 `CASE` 条件（向前推进）下成对替换，保证已认证页链 `previous_digest` 始终可验证，不出现"页码前进、摘要过期"的组合。语句级写入不跨越传输 I/O。

并发语义：同机两个连接处理不同 sync job 属正常重叠，持过期快照的写入对已前进的活跃行是安全 no-op，必须表现为无 storage_corrupt、无不健康 status；而"全新读取到的回退 head"（`pages < observed`）仍按原样拒绝并记入 `sync_index_errors`，公开于 `sync.status.transport_errors`。消费前缀游标只前进，已消费页文件隐藏后继续同步不得重读或报错。

## 测试钩子边界

确定性双连接回归使用 `#[cfg(test)]` 线程本地调度门（index 发现 prior 读取后的暂停点），仅测试构建编译、生产零痕迹；公开行为断言（sync.run/process、sync.status、clothing.list、回退拒绝、隐藏页继续同步）为验收主体，钩子不替代公开断言。

不改数据库 schema/版本，不改类别（category/quarantine）逻辑。

公开回归位于 `crates/wardrobe-core/tests/sync.rs` 的两个 epoch 图片测试，以及 `crates/wardrobe-core/src/sync/cursor_contract.rs` 的并发测试。原始红/绿运行证据保存在开发工作区外部的 `wardrobe-execution/results/resume-sync-zcode/`，不属于发行包。旧扁平兼容回归搬移 commit 后保留其索引引用的 chunk 原位置；全扁平 chunk 路径仅经源代码审查。个人图、试穿图的独立负例尚未补充。
