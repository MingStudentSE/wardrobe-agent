# #27 同步交接

#26 实现本机加密与设备信任。远端传输、同步冲突、outbox 与媒体选择由 #27 实现，不能把本票的本机设置当作已上线同步。

可信本机 Application 入口：`encryption.create/status/restore/password.change/unpair`；配对依次为 `pair.offer → pair.accept → pair.confirm → device.apply`；撤销为 `device.revoke → device.apply`。以上均带 `encryption.` 前缀，全部禁止外部客户端调用，MCP 不列出。CLI JSON 输入只允许 stdin。内部同步应调用 Application 的密钥处理边界，不返回 master 或设备私钥。

`encryption.encrypt/decrypt` 接收 `kind`（changes / blobs / snapshots / device-state），明文输入/输出为 `plaintext_base64`，密文为 `envelope`。协议1固定 XChaCha20-Poly1305；AAD 是 UTF-8 JSON 数组 `["wardrobe-e2ee",1,space_id,epoch,kind]`，密钥再经 HKDF 按空间、epoch、用途派生。nonce 为24随机字节；密文包含16字节tag；协议、长度和认证错误必须拒绝。

设备 manifest 为 `{payload_base64,signature}`；签名验证原始解码字节，成功后才解析 JSON。接收者必须使用邀请中 pin 的创建设备 Ed25519 公钥，不能接受后端指定的新信任根。manifest 内含 protocol_version / space_id / epoch / revision / owner_public / devices / recipients；每个 recipient 的 epoch 钱包通过独立临时 X25519 DH 封装。低阶 DH、错误签名、空间错配、旧 revision 均拒绝；相同已应用包允许幂等重放。

pending 第二设备没有主密钥。邀请10分钟有效，创建设备数据库 IMMEDIATE 事务单次消费回执。确认后传 manifest，第二设备 apply 后激活。配对秘密须走可信渠道，不能与邀请密文一起上传同一未经认证后端。

撤销生成新 epoch，保留设备收到新 manifest 才能使用新钥。旧设备已知历史钥无法远程擦除。同步写入必须校验当前受信任 manifest 和 epoch，不能仅因旧 epoch 密文仍能解密就接受撤销设备的新提交；离线状态不能宣称全网撤销完成。

每台设备恢复包封装自己的私钥身份与当时钱包。恢复码必须单独保存。pending 恢复包或轮换前的旧包恢复后需要最新受信任 manifest 才能获得新增 epoch；密码变更不撤销已保存旧包。创建者恢复签名身份后才能继续管理设备。新配对导入空间 UUID 只允许没有衣物/搭配/导入/穿着/资产事实的目标空间；成员事实与其 UUID 由 #27 正常同步，不得自行映射已有个人事实。

OS仅保存本机 credential_id 对应固定32字节解锁钥，SQLite 中的 wallet_wrap 是本机密文，包含设备私钥及历史空间钥。后续钱包和状态同 SQLite 事务更新。恢复安装生成新 credential_id；初次安装提交失败删除新OS条目。解除配对删除本机OS钥与状态，保留衣橱事实。不得远端导出本机数据库、wallet_wrap、credential_id、客户端token或模型凭据；原图不进入同步候选。
