# ADR 0028：S3-compatible 同步后端与 ETag 条件写协调

状态：已实施；ticket-29 分支 `codex/ticket-29-s3`，红/绿证据在 `wardrobe-execution/results/continue-20261007/ticket29/`。

## 背景

#29 要求在 #27 的统一 `SyncStorage` 契约上落地 S3-compatible 后端：endpoint/region/bucket/prefix/path-style 配置与安全凭据可操作，真实 S3-compatible 服务器完成双设备往返；共同契约覆盖分页/流/条件竞争/截断/认证/限流/重试/删除/收敛，且 ETag 不当内容 hash。#27 的本地文件夹适配器（staging+rename+fsync、CreateOnly、锁边车、游标单调、截断零应用、白名单、加密协议）保持不变。

## 传输适配器（`sync::storage_s3::S3Objects`）

- 协议：手写 AWS Signature V4 + `reqwest` blocking（无 SDK 依赖；新增 `hmac 0.13`，与既有 `sha2 0.11` 同族）。对象路径按段 RFC3986 编码，查询参数排序后编码并纳入签名；`Host` 以实际请求 authority 参与签名，path-style 与 virtual-host 两种寻址都支持。
- 分页列举：ListObjectsV2 `start-after` + `max-keys`，游标是单调递增的对象键（与本地文件夹同语义）；`encoding-type=url` 解码键名。limit 1–1000 之外与非法游标（不匹配前缀/非法键）映射 `invalid_cursor`/`invalid_object_key`，与本地一致。
- CreateOnly：`If-None-Match: *`；MatchVersion：`If-Match: "<etag>"`。构造适配器时做真实能力探测：先 CreateOnly 发布探测对象，再重复 CreateOnly——服务器若返回 200（忽略条件头）或 501/405（不支持）即以 `storage_unsupported` 明确拒绝，绝不静默改写发布语义。探测对象随后清理。
- ETag 只作条件写协调的版本标识；完整性由同步协议自身的 sha256（对象→marker 两步发布 + 读取校验）承担。响应体短于声明的 Content-Length（传输截断）在读取期报错，部分字节不可见；单对象上限 2MB 与本地一致。
- 条件删除按服务器实际能力分派（构造期探测：用陈旧 If-Match 尝试 DELETE）。实测 MinIO 静默忽略 DELETE 的 If-Match（对陈旧版本返回 204 并删除现值）——该行为下适配器对 `MatchVersion` 删除明确返回 `storage_unsupported`（既定降级：宁可拒绝也不移除新发布的版本）；支持 If-Match DELETE 的服务器（如 AWS S3）自动获得 412→`storage_precondition` 的完整语义。无条件删除 `Any` 全后端一致（对象不存在 → `storage_not_found`，与本地 `remove_file` 对齐）。
- 错误映射（类型化）：401/403→`storage_auth`；404→`storage_not_found`（条件写中指向已消失的版本→`storage_precondition`）；412→`storage_precondition`；501/405→`storage_unsupported`；5xx/网络错误/超时→`storage_transient`；3xx（endpoint/region 配错）→`storage_transient` 可见失败，不跟随重定向。错误消息只引用 S3 错误码 `<Code>`，不回显服务器消息，避免敏感信息进入日志。
- 重试预算（父规格）：网络/超时/5xx（含 503 SlowDown）首试后最多 2 次自动重试，遵守 `Retry-After`（上限 30s 为同步批次设界），无该头时 200ms 退避；预算耗尽后映射 `storage_transient`。凭据的 Debug 输出一律脱敏。
- 键契约：`storage::valid_object_key` 抽为 LocalFolder/S3 共享（拒绝驱动器名、绝对路径、非 Normal 分量），保证切换 adapter 时哪些键合法不变。

## 配置与凭据

- `sync.configure` 新增 `provider_kind:"s3"`：`endpoint`（http/https，含端口）、`region`（默认 us-east-1）、`bucket`（小写字母/数字/点/连字符）、`prefix`（适配器 key 根，缺省为空）、`path_style`（默认 true）加 `access_key_id`/`secret_access_key`。S3 分支拒绝 `directory`，本地分支拒绝 S3 字段（防错配）。
- 凭据只进 OS 安全存储：keyring 服务 `wardrobe-agent.sync`，账户为目标键（连接参数+space 的摘要）；`sync.status` 与配置输出只含非密连接参数。CLI 经 `--input-stdin` 传密，不进命令历史与进程参数。
- 配置即验证：configure 时构造适配器做能力探测与认证校验，失败则整次配置不生效（不静默半配置）；`sync_step` 每批从 target 文档构造适配器（S3 凭据从 keyring 读取，缺失映射 `storage_auth`）。
- 发布标记不依赖原子 rename：S3 单对象 PUT 与 head 的 `If-Match` CAS 原子成立，#27 的「对象先于 marker 可见」规则原样成立；`capabilities()` 返回 `atomic_publication:true`。
- GUI 设置页同步卡片增加目标类型选择（本地文件夹 / S3 兼容对象存储），S3 表单含 endpoint/region/bucket/prefix/path-style 复选框与两组密钥输入（password 型）；关键操作全部键盘可达，不依赖 hover/拖拽。

## 测试与证据

- 共同契约参数化：`tests/sync_storage_s3.rs` 把同一套断言跑在 LocalFolder 与真实 MinIO 适配器上（往返/版本语义/删除、分页游标、可移植键、条件写、2MB 上限、并发条件竞争唯一赢家）；S3 专属用例覆盖认证失败→`storage_auth`、缺失 bucket→`storage_not_found`、503 SlowDown 经本地真实 HTTP 代理注入（吸收 2 次注入的 503+Retry-After，预算耗尽→`storage_transient`）、GET 响应体截断整体失败、并发 CreateOnly 单赢家。本地代理注入的是真实协议字节流上的受控故障，不是协议 mock。
- 适配器等价：`tests/sync_s3_dual_device.rs`（ignored，真实 OS 钥匙串 + 真实 MinIO）——同一 fixture（成员、单品、采用展示图、搭配、跨端穿着）分别经本地文件夹与 S3 完成真实双设备同步，剥离安装本地的身份/时间戳/路径后领域事实逐字段相等；另验证 S3 配置往返、secret 不出现在任何输出、远端对象字节无凭据/原图泄漏。
- 真实 CLI 证据：`results/continue-20261007/ticket29/cli-evidence/run-dual-device-s3.sh`（28 PASS / 0 FAIL，exit 0）——脚本自建自停真实 MinIO（源码构建，生产级 S3 实现），两个真实数据目录、每步独立 `wardrobe-cli` 进程：配对→S3 配置（stdin 凭据）→发布→接收→截断远端对象整体失败→修复字节收敛→双端幂等→白名单与凭据扫描。
- 回归：#27 的 5 个本地双设备 ignored 回归与索引 cursor 并发回归全部保持绿；全量 `cargo test -p wardrobe-core` 除 `backup_pair_projection` 的 2 个失败外全绿（该失败在未改动基线 8a90059 上同样复现，属既有债务，与本票无关）。

## 边界（留给后续 ticket）

暂停/重试/更换目标/密钥生命周期与 tombstone 回收属 #31；WebDAV 后端复用同一契约（#30）；乱序/时钟偏差等深度冲突归 #28；GUI 端到端点击验收归 #34/#35。
