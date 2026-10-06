# #13 独立合并审查

从 root4fa138f 独立合并实现候选96faf05，再整合已独立审查的#21 eb1c04f。保留 recognition/tryon/flatlay/encryption 的模块、客户端测试和各自已提交任务防重复发送分支。DB最大版本12；所有实际表迁移按存在性执行，不因全局版本跳过 encryption、recognition、wear历史表。

审查 F05/F30 与 ADR0014：选定provider/图片IDs/原图或展示图角色/源哈希/基线冻结，PNG传输哈希和尺寸另存；明确上传，不把模型原话或词汇候选当事实。建议与事实独立，审核时完整基线CAS，审核事实/audit/outbox/建议状态同事务；重分析保护后续人工修改。提交前持久标记、未知结果不盲重发，429有限退避，取消不能被成功或429响应复活。

发现并修复 POST前缺租约到期检查。真实SQLite触发器在claim后使lease到期，外部HTTP fixture观测：旧代码仍POST1次，修复后POST0次且submission保持not_submitted。最初fixture接受流继承非阻塞导致read WouldBlock，属于测试装置错误；修复装置后重新运行红证据（merger-expiry-red.log），不以装置失败证明产品问题。最终联合8项recognition通过。guard仅修改recognition，未替换共享图片adapter已有守卫。

另一审查改善：每条建议显示创建时的图片数量、原图/商品展示图版本与短资产ID；切换当前发送选项不会改写旧建议来源。GUI实际断言切换到original后旧proposal仍标商品展示图。320/1280审核截图及实际鞋图截图重新生成，320与实际图已独立人工查看；7宽度无横向溢出。

实际环境macOS ARM64，root独占target，CARGO_INCREMENTAL=0 / j2 / DEV_DEBUG=0 / TEST_DEBUG=0；PKG_CONFIG=/opt/homebrew/opt/pkgconf/bin/pkg-config，PKG_CONFIG_PATH=/tmp/wardrobe-media-native/install/lib/pkgconfig，DYLD_LIBRARY_PATH=/tmp/wardrobe-media-native/install/lib。使用候选+修复+#21同源CLI重新编译，没有调用09/07缓存产物。

- cargo test --workspace --locked：119 passed / 8 ignored；完整保留历史/加密、商品图、搭配图、试穿、识别及真实CLI/MCP测试。
- DB12识别合并后显式真实Keychain `encryption --ignored --test-threads=1`：6 passed；随后联合#21 GUI也实际执行建立/密码更改/解除/恢复/重启。未读取用户真实密钥。
- cargo clippy --workspace --all-targets --locked -- -D warnings、cargo fmt --all -- --check、npm build、git diff --check通过。Tauri cargo metadata --locked --no-deps通过，只说明锁文件，不冒充原生窗口操作。
- WARDROBE_ENCRYPTION_OS_TEST=1、WARDROBE_RECOGNITION_REAL_ENDPOINT=http://127.0.0.1:18083/v1 完整GUI：38项中36 passed /2 skipped（显式抠图/SmolVLM连接票据门槛）。本票真实SmolVLM识别与Keychain均执行，#21真HTTP协议fixture也执行；不存在将fixture记成付费云推理。

独立实际CLI重新启动既有SmolVLM256M CPU2/ctx4096 runtime，公开许可shirt/shoe/jeans三个样本均实际推理成功，所有proposal仍pending_review，事实与提交基线一致，传输PNG来源可查；merger-actual-cli.json及reproduce_merger_actual_cli.py保留复现。鞋描述黑白鞋带、裤子蓝牛仔裤可用；白衣再次猜测IMUA品牌，原话完整保留，不宣传准确率，人工纠错仍必需。模型服务在CLI/GUI终态后停止，权重和运行时保留。

本报告不缩减#35三平台原生安装验证，也不证明#14/#20/#21实际付费云质量。当前新CI受账户付款/额度限制，见integration-ci-account-block.md；旧9070完成CI不能替代新代码验证。无push或issue关闭。
