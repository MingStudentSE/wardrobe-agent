# #26 本机验收

2026-10-07，macOS；代码已合入 root b08746b，未 push，未关闭 issue。真实 SQLite / OS Keychain / Application / CLI / MCP / GUI 均用于本票验收，使用临时生成凭据和合成字节，不使用个人衣橱或用户密钥。

- `cargo test --workspace`：95通过，7 ignored。ignored 中本票5项另行显式运行，另2项属于模型凭据票据。
- `cargo test -p wardrobe-core --test encryption -- --ignored --test-threads=1`：5通过，真实 Keychain；多实例配对与重启、pending无密钥、签名篡改拒绝、确认单次消费、三设备撤销轮换、恢复/改密码、并发确认、SQLite/WAL不含明文私钥与恢复秘密。
- 普通加密测试验证字节往返、错密码、错空间/用途/版本、nonce长度、截断密文、认证失败和随机nonce。外部full客户端伪member参数仍不能操作空间秘密。
- CLI入口回归含强制stdin与真实MCP工具过滤/拒绝；最新源`cargo build -p wardrobe-cli`后运行GUI，无旧二进制替代。
- `npm run build`、`cargo clippy --workspace --all-targets -- -D warnings`、`git diff --check`通过。
- `WARDROBE_ENCRYPTION_OS_TEST=1 npm run test:e2e`：31通过，2 skipped（真实抠图模型/SmolVLM票据环境）；本票2项均执行，实际创建、密码变更、解除、恢复、重载密钥不再展示。7宽度320/375/414/768/1024/1280/1440无溢出；320和1280截图已人工检查。

红绿过程：最初 Application 加密入口不存在；随后两阶段配对的pending加密测试揭示邀请接受即获主密钥，修复为确认后激活；外部full维护入口由invalid_input修正为scope_denied。最终回归日志在本目录。后补的并发与数据库隐私测试作为真实边界回归，不声称其先于实现。

Windows真实 Credential Store（2560字节上限）、macOS Keychain、Linux解锁Secret Service已加入五平台CI的显式OS加密测试步骤。此报告只证明本机macOS结果；新代码五平台CI需父任务push后确认，不能用默认ignored或旧CI当作通过。

协议与安全边界见ADR0012和[sync-handoff.md](sync-handoff.md)。本票不包含远端上传或同步实现，撤销已知历史密钥不能擦除，密码变更不能废止用户保存旧包。新恢复/轮换材料需重新保存；第二设备恢复需最新可信manifest。未进行第三方密码协议审计，不把依赖库审计冒充产品协议审计。
