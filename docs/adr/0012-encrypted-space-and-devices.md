# ADR 0012：加密空间、恢复与设备信任

状态：实施中，#26；不代表#27远端同步或#35安装验收完成。

单空间的原始 workspace UUID 是加密空间身份，第二设备只在空空间接受不同空间身份，不能偷偷重标已有事实。DB10创建实际 encryption_state / encryption_pairings 表，不按全局版本跳过表创建。加密状态、客户端授权、OS secret 均是本机状态，#27不得直接导出数据库。

使用标准库 RustCrypto XChaCha20-Poly1305 0.10.1、HKDF-SHA256 0.13、Argon2id 0.5.3；库官方文档及参数见 docs/verification/ticket-26/plan.md。系统 Keychain / Credential Store / Secret Service 仅保存每个本机安装的32字节随机解锁钥（base64 44字节）；空间密钥、设备X25519私钥、创建设备Ed25519签名私钥和历史epoch密钥进入AEAD钱包密文，与设备状态在同一SQLite IMMEDIATE事务提交。SQLite不存在明文私钥或解锁钥。初次安装先写OS钥，再提交密文行，提交失败清理OS钥；后续改密码、轮换和设备状态更新不改变OS钥，避免跨存储原子性裂缝。恢复为新本机credential_id登记新的解锁钥，不覆盖同OS另一安装的凭据。内存包装采用Zeroizing，钱包Value释放时清理字符串；这不是对整个进程/操作系统内存的机密性保证。

密码KDF固定Argon2id v19、65536KiB、3轮、1lane、16字节OS随机salt，输出32字节，输入不能控制资源参数。独立恢复密钥为32字节OS随机secret，只明确返回一次。密码变更重新包装同一密钥，不重加密媒体；用户保存的旧恢复包仍含旧密钥，不能通过改密码抹除旧包或已经知道的密钥。撤销另一个设备时生成新epoch主密钥，保存旧钥供合法设备读取历史，并给保留设备通过X25519临时DH+HKDF+AEAD单独封装所有合法epoch钥。恢复包包含本机钱包与身份的密文，恢复密钥不可与公开密文包一起放到同步后端。

配对为两阶段：创建设备生成10分钟邀请与256bit秘密；第二设备认证邀请后保存pending设备私钥，生成加密回执，尚不能读写加密数据。创建设备在IMMEDIATE事务内验证并单次消费回执，签发完整设备manifest。第二设备验证Pinned Ed25519创建设备公钥及严格签名后解封仅属于其X25519公钥的密钥包，才能激活。邀请秘密通过用户可信渠道转交；未经认证的后端不提供信任根。

设备manifest包含协议、空间、递增revision、epoch、设备公钥及各设备密文包，整体由创建设备Ed25519严格签名，防止被撤销设备拿旧空间密钥伪造新设备名单。旧revision不可回滚；同一已应用manifest允许幂等重放。撤销设备不在新manifest recipients内，拒绝新钥并将本机标记revoked；旧设备已经知道的历史资料不能远程抹除。#27必须传播并验证manifest、对当前epoch写入校验、在离线或未收到撤销时准确表述状态；不能把本机列表直接称为服务器上永久封禁。

所有 encryption.* 都是可信本机维护入口，外部full/limited客户端也不能以伪member_id绕过。CLI敏感输入强制stdin，MCP不提供私钥管理工具。后续同步业务应在共享Application内部调用密钥，不通过公开API返回master/private secret。公开密文字节工具不是远端上传器；#27须从已验证fact/outbox/规范化display资产构造payload，不能让original、模型凭据、本机token或任意文件成为同步候选。

Microsoft [CREDENTIALW](https://learn.microsoft.com/en-us/windows/win32/api/wincred/ns-wincred-credentialw) 限制CredentialBlob为2560字节，因此不能把随设备/轮换历史增长的钱包JSON直接放入Windows安全凭据。固定44字节OS解锁条目避免这个边界，AEAD密文钱包仍保留完整合法历史钥。公开status会删去本机credential_id/wallet_wrap；两者不属于远端协议。
