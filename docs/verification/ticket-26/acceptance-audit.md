# #26 功能验收与发布边界

主代理未实现本票；本轮复核 GitHub #26 四项 criteria、F21、ADR0012、独立合并报告及当前集成的实际测试日志。当前 root a83e3fd 业务源码与 #21 独立验证 beefa47 相同；后续只追加验收文档。

| 功能要求 | 已运行证据 |
| --- | --- |
| 建立空间、第二设备配对、恢复与安全存储 | 独立 GUI 真实 Keychain 创建/更改密码/解除/恢复/重载；六项真实 OS 测试包含第二个独立 SQLite 安装、重启、pending 无密钥、确认单次消费与并发竞争。见 ticket-21/independent/encryption.log 及 gui.log。 |
| 真实密文字节往返，错钥/截断/篡改/协议/nonce/AAD 拒绝 | 当前 workspace 实际执行 encrypted_envelopes_roundtrip_and_authenticate_workspace_kind_version_and_bytes；另行六项 OS 测试验证轮换后合法设备读取历史 epoch、已撤销设备不能取得新 epoch、恢复及失败事务原子性。不是自制密码函数或默认 ignored 代替通过。 |
| 算法参数与审查、供后续同步使用的应用边界 | ADR0012 固定 RustCrypto XChaCha20-Poly1305、HKDF、Argon2id 参数；独立审查确认签名原始字节验证、manifest 防回滚、OS 32 字节解锁钥与 SQLite AEAD 钱包同事务。seal/open 边界传递认证密文，不返回 master；数据库/WAL 明文私钥扫描实际通过。 |
| 公共行为与领域记录 | 当前真实 Application/SQLite、CLI stdin/MCP 禁止私钥工具、GUI 构建与回归通过；GLOSSARY、ADR0012、实现及独立报告齐备，无占位能力。 |

原始独立 #26 运行九项加密测试全部通过，最新 #21 集成另行运行六项真实 Keychain 测试全部通过，三项非 OS 加密测试在默认 workspace 中执行。当前 Windows Credential Store 与 Linux Secret Service 的新钱包路径尚未实际运行：新 CI 在 runner 启动前被账户限制拒绝，不能计为通过。此缺口属于完整三平台 Beta 发布验证，#35 保持阻断，不以 macOS 证据声明全平台可用。

#26 是本机加密与信任边界，不是远端同步传输：目前没有远端业务上传器。#27 必须实际从经过白名单验证的事实/outbox/展示资产产生密文，拒绝原图、凭据、本机 token 与钱包；不得把本票的 seal/open 测试当作远端同步或远端原图排除的已运行证明。远端无明文/无原图的最终运行验收仍由依赖本票的 #27/#28/#29/#30 及 #35 完成。同步接收必须验证当前签名 manifest、epoch 与设备撤销，不能只靠能够解开旧密文判断权限。

本机加密行为证据充分，但 #26 第三项明确包含远端无明文/无原图；当前证据未实际覆盖远端上传。不能仅把这一条交给后续任务就宣称本票所有 criteria 已通过，本轮保持 #26 OPEN，#27 尚不能执行。需要继续厘清任务边界并补足与本票要求匹配的实际外部存储验证，而不是利用未上传这一事实作为验收。F21 最终跨设备同步及 F24 三平台发布要求保留。密码变更不能擦除已知旧密钥，撤销不能擦除已下载历史，第三方产品协议审计未进行，这些边界保留在 ADR 与发行记录。
