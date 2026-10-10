# ADR 0038：WebDAV 传输适配器（条件写入、能力探针与凭据隔离）

状态：已实施；分支 `codex/ticket-30-webdav`（基于集成 tip 8a90059），红/绿证据在 `wardrobe-execution/results/continue-20261007/ticket30/`。

## 背景

#30 要求配置 WebDAV 并完成真实双设备同步，复用 #27 的同一套 `SyncStorage` 契约与验收。本票只新增传输适配器与配置面，不改动同步引擎、加密、白名单与合并边界；`storage.rs` 的 trait 定义零修改（仅 `pub use` 新适配器），与并行的 #29（S3）互不依赖。

## 传输映射

- `PROPFIND Depth:1` 列举映射为客户端排序分页枚举：服务器无原生分页，适配器按完整键排序、返回严格大于游标的前 `limit` 项并给出下一游标，与 `LocalFolder` 的游标契约逐字相同（含 `invalid_cursor` 校验）。
- 条件写全部由服务器判定：`CreateOnly` 用 `If-None-Match: *`；`MatchVersion(etag)` 用 RFC 4918 `If: <绝对URL> ([etag])` 状态令牌（弱比较，同时兼容 mod_dav 在 GET/PROPFIND 之间强弱形式漂移的 ETag）。**ETag 只是条件写的透明令牌，绝不当作内容摘要**；内容完整性仍由引擎的 sha256 发布标记（#27 Publication Marker Gate）判定。
- 不使用 MOVE、不依赖服务器原子改名：发布可见性复用 #27 的 object→`.commit` 标记两步 `CreateOnly`；`capabilities().atomic_publication` 如实声明为 `false`。
- 父目录逐级 `MKCOL`（405=已存在视为并发竞争的合法结果）；键校验与 LocalFolder 同规则（相对 POSIX 分段、无盘符/反斜杠/控制字符）。GET 全程流式；PUT 在协议上界内缓冲（2 MiB，与 LocalFolder 的 `take(2 MiB+1)` 一致），超限在发送前以 `storage_corrupt` 拒绝、不发布任何字节。
- 传输已知的真实缺口：mod_dav 的弱 ETag 由 (size, mtime) 派生，同秒同长度覆盖可能得到相同 ETag；正确性不依赖"覆盖后版本必变"（每个发布者只有自己的 `head.index` 命名空间，游标单调由引擎保证），但同一文件同秒同长度互覆盖的窗口由服务器 412 判定能力兜底，本地文件夹（sha256 版本）无此窗口。

## 类型化错误与重试预算

401/403→`storage_unauthorized`（不重试）、404→`storage_not_found`、409/412→`storage_precondition`、423→`storage_locked`、429/503→`storage_rate_limited`（遵守 `Retry-After`，封顶 30s）、其余 5xx/网络/超时→`storage_transient`。自动重试预算按父规格：网络/限流类最多 2 次（共 3 次尝试）；幂等操作（GET/HEAD/PROPFIND/MKCOL/DELETE）与条件写可自动重试（条件头在服务器端重新判定，语义安全），无条件写（`Any`）不自动重试（结果不明确）。条件写重试遇到 412 时以远端内容一致性区分"首次已完整落地"与真实竞争，避免重复发布的假阳性冲突。

## 能力探针与明确拒绝

`WebDav::connect`（配置期与每个 worker 批次）执行真实条件写入握手：创建探针对象 → 重复创建必须被 412 拒绝 → 过期条件写必须被 412 拒绝 → 清理。任何一步被无视（实测 dufs v0.46.0 无视条件头）即返回 `storage_unsupported` 并拒绝配置/同步——**不静默降级**，多设备互相覆盖的风险在配置期就可见。

## 凭据与配置

`sync.configure` 新增 `provider_kind="webdav"`：输入 `url/root/username/password`。目标文档（SQLite `sync_targets`）只存连接描述（url/root/username/target_key），密码立即写入 OS 安全存储（服务 `wardrobe-agent.sync`，账户为 target_key），不进入目标文档、审计、日志或 argv（CLI 走 `--input-stdin`）。再次配置同一目标可省略 password（从钥匙串取回）；worker 每批从钥匙串读取。GUI 设置页同步卡片新增目标类型切换（本机文件夹 / WebDAV），密码输入框不回显、保存后清空；reload 不覆盖用户正在编辑的表单。

## 测试与证据

- 适配器契约（真实 Apache mod_dav，`tests/sync_storage_webdav.rs`，8 个 ignored 测试）：分页/排序/游标、条件写与 ETag-非-hash、键校验与根转义、流式往返、CreateOnly 并发唯一赢家、LOCK/423 类型化、dufs 能力缺失明确拒绝、认证失败不烧重试预算、限流 `Retry-After` 与恰好 2 次自动重试（代理计数断言 3 次尝试）、超时/断网→transient、截断 GET 如实交付且绝不以 ETag 校验内容。
- 故障注入走本地 TCP 代理（`tests/support/webdav_fault_proxy.py`）注入真实 503/挂起/截断，服务器字节不动。
- 双设备端到端（`tests/sync_webdav_dual_device.rs`，ignored）：真实配对 → 配置（密码不入文档）→ 发布/接收 → 服务器端截断整体失败不误应用 → 修复后同一日志收敛 → 重复同步幂等 → 双向回传 → 远端目录白名单扫描（口令/成员名/单品名明文零命中）。
- CLI 证据：`results/continue-20261007/ticket30/cli-evidence/run-webdav-dual-device.sh`，26 项全过（exit 0），两个真实数据目录 + 真实 Apache（127.0.0.1:5982）+ 真实 CLI 进程。
- GUI：`apps/desktop/tests/sync-webdav.spec.ts`（需 `WARDROBE_WEBDAV_GUI_TEST=1`）真实服务器上完成错误密码 401 提示、正确保存、状态无密码、窄屏不溢出；既有 `sync.spec.ts` 两个用例（含 OS 钥匙串双安装用例）保持全绿。
- 回归：`sync_storage`（LocalFolder 契约）、`sync_dual_device`、`sync_jobs`、`registry_contracts` 全部通过。

## 边界（留给后续 ticket）

S3 后端（#29）复用同一契约，各自实现能力探针；暂停/重试/更换目标/密钥生命周期归 #31；GUI 端到端验收归 #34；密钥轮换后 WebDAV 目标的凭据迁移策略暂未实现（更换目标需重新输入密码）。
