# #26 独立合并验证

候选 7ba821a 从 root b08746b 独立合并到 codex/merge-26，并整合 #20（19faa26）。保留 encryption / flatlay 模块和两票 CLI/MCP 行为测试；DB 保持实际最大版本10，encryption 两表每次按存在性修复。后续 root 纯证据提交另行合并，不改变测试源码。

独立审查：OS仅存32字节随机 unlock（base64 44字节），SQLite AEAD钱包保存空间/设备/签名私钥和历史epoch；更新钱包/状态使用同一个 IMMEDIATE事务。初装提交失败清理新OS条目；外部full客户端也拒绝 encryption.*，MCP工具过滤，CLI强制stdin。manifest认证原始解码字节后解析，使用邀请pin的Ed25519公钥与verify_strict，拒绝回滚；pending设备不获master，创建设备确认事务单次消费。撤销轮换只给保留设备封装新epoch；不宣称能擦除撤销设备已知旧数据。恢复/改密码保留合法历史钥。后续远端同步必须验证当前manifest/epoch，不能导出本机wallet/凭据/原图。

追加独立行为断言：高版本数据库缺encryption表时修复且不降级；轮换后保留设备可读旧epoch，旧manifest拒绝，相同已应用manifest幂等；SQLite UPDATE故障注入使撤销整体失败，状态/epoch保持原值；成功轮换及改密码后，恢复设备同时读取epoch1/2真实密文。

实际命令环境：macOS ARM64；CARGO_TARGET_DIR=/Users/links/.codex/worktrees/9729/电子衣橱 Agent/target，CARGO_INCREMENTAL=0，CARGO_BUILD_JOBS=2，CARGO_PROFILE_DEV_DEBUG=0，CARGO_PROFILE_TEST_DEBUG=0；PKG_CONFIG=/opt/homebrew/opt/pkgconf/bin/pkg-config，PKG_CONFIG_PATH=/tmp/wardrobe-media-native/install/lib/pkgconfig，DYLD_LIBRARY_PATH=/tmp/wardrobe-media-native/install/lib。临时target链接仅服务开发测试，不改变release缓存。

- cargo test --workspace --locked：102 passed / 8 ignored；其中本票6 OS测试另行实际运行，另外2项是模型票据门槛。
- cargo test -p wardrobe-core --test encryption --locked -- --include-ignored --test-threads=1：9 passed / 0 ignored，真实Keychain与临时随机密钥，未读取用户真实凭据。
- cargo build -p wardrobe-cli --locked：来自本合并源码；随后 WARDROBE_TEST_PORT=5196 WARDROBE_ENCRYPTION_OS_TEST=1 npm run test:e2e：32 passed / 2 explicit模型gates skipped。实际GUI→CLI→Application，包含加密建立/密码更改/解除/恢复/重启和#20多图协议；7宽度无溢出，320/1280截图独立人工查看。
- cargo clippy --workspace --all-targets --locked -- -D warnings、cargo fmt --all -- --check、npm run build、git diff --check 通过。日志为本目录 merger-*。

仅证明本机macOS和开发GUI链路。#26新代码五平台CI尚未push执行，不将旧CI或默认ignored当成完成；#20真实付费云生成仍未验收。未进行第三方产品密码协议审计。无push、issue关闭或远端同步宣称。

父任务复核后将设置文案改为“空间密钥由本机系统安全存储保护”，与OS解锁钥/加密钱包架构一致。仅修改文字后重新实际执行 encryption.spec.ts：2 passed（真实Keychain），重拍七宽度截图并人工查看320宽，npm build重新通过。Tauri cargo metadata --locked --no-deps通过，只验证lock，不冒充新原生打包。
