# #26 实施与验收边界

Application API +真实临时SQLite是已确认的核心测试边界；CLI/MCP仍调用同一Application。GUI使用既有真实loopback测试桥和Atelier视觉语言。

选择纯Rust RustCrypto chacha20poly1305 0.10.1（XChaCha20-Poly1305，32字节密钥，24字节OS随机nonce，16字节tag）、hkdf 0.13 / SHA256用途派生、Argon2 0.5.3（Argon2id v19，64MiB、3轮、1lane、16字节随机salt）。固定版本和参数，输入不能提升KDF资源；严格认证协议、空间、epoch、用途，nonce和ciphertext用base64编码。恢复码是32字节OS随机secret，不是用户密码；独立包装主密钥。主密钥只进OS安全存储。SQLite保存公开协议/设备状态和密钥密文，远端公开格式不得包含用户事实、原图、客户端token、模型凭据。

参考：[RustCrypto ChaCha20Poly1305 0.10.1](https://docs.rs/chacha20poly1305/0.10.1/chacha20poly1305/)、[HKDF 0.13](https://docs.rs/hkdf/0.13.0/hkdf/)、[Argon2 0.5.3](https://docs.rs/argon2/0.5.3/argon2/)。库公开审计不能替代本项目协议审查。

配对使用带到期、单次接受、显式设备标识的256bit邀请秘密，密钥包在本机生成加密后传递。配对秘密与密文包必须通过用户可信渠道转交。撤销要求未来epoch轮换，旧设备已经知道的旧密钥不能远程抹除。#27实现传输层时必须传播认证的设备与epoch状态，不将撤销设备仅靠本机列表宣称全网阻断。密码变更重新包装，不重加密已存媒体。轮换后的恢复包需重新保存。

待完成：加密往返与篡改、恢复、可信设备配对、OS存储、重启、race、撤销、UI和CLI/MCP真实验证。当前计划不是验收证据。
