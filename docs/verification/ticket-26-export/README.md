# #26 应用密文文件包补充验证

实现终态，等待独立merger审查；未push、未关闭Issue。业务源码验证点d52f6eb2bcc7ddb5ea30115bfbd29f527a624bbb，已合care13、insights14及root52b4ae3；最终仅补证据和最新纯文档，不降低DB14，不新增表。ADR0019说明协议与容量，sync-handoff.md约束#27复用。

Application在真实数据库一致快照内显式选择事实字段和当前／历史采用展示图，kind分离HKDF+AEAD后写入独立文件夹。真实两CLI安装已配对，第二安装验证／解密事实与展示字节，**不导入衣橱**。实际外部三文件扫描未见事实名称/备注、源文件名、原图字节或本机路径。读取保留全部care3修订、配置及图片引用；持久89天偏好保留身份/revision，peer仍默认90且没有衣橱事实。actual-cli.json与reproduce_cli.py记录复现，所有钥只从新临时安装生成、通过stdin传递并清理，无用户真实钥查询。

终态：workspace **140 passed /11 ignored**；另外真实OS **9 passed**（6既有钱包/配对/恢复/DB-WAL私钥扫描＋3package场景），不是将ignored计为通过。完整GUI **42 passed /3 skipped**（45项），使用同源CLI→Application真实开发桥；三项未启动的本地cutout/SmolVLM连接/实际识别测试保留先前#7/#12/#13独立真实证据，本轮不冒充再次运行。Clippy workspace/all-targets -Dwarnings、fmt、locked metadata、同源CLI构建与tsc/Vite构建均exit0。没有实际付费云API或原生Tauri picker弹窗验证。

真实红绿：无package.export时unsupported_operation；旧outfit非主图移除后expected2 got1；care未列入时Null vs3；持久insights_preferences未列入时Null vs89。共享资产白名单保留全部旧搭配图片、wear/care历史授权，绝不放开原图/未采用预览。4个包公共行为含1默认full客户端拒绝＋3实际OS场景；另外验证损坏/25MiB外部文件/目录symlink/签名tamper、源展示图损坏无原图回退、当前epoch/已知撤销、1700真实单品×10000备注超限0文件发布且本地事实保留。File读取严格take(cap+1)，Unix额外核对stat/open dev/ino；并不声称重现了对抗TOCTOU竞态。

GUI设置页有目录picker与触控输入、明确导出/验证按钮；操作与picker期间防并发，busy/error/success可见。导出长路径提示及验证提示在320/375/414/768/1024/1280/1440检查无横向溢出，实际查看320截图。MCP授权tools/list实际无encryption工具；full客户端两个新操作实际scope_denied，可信本机CLI强制stdin。

接收先验证当前raw签名清单、space/epoch/revision与发送设备授权。未知撤销的离线peer仍可读取已有旧包；接受撤销清单后必须拒绝，不声明远程擦除。单手动包容量超限失败、不截断；#27仍需分页/分片、transport/outbox/inbox/merge覆盖完整规模。本证据是**实际独立文件系统边界**，不是S3/WebDAV、互联网服务或完整同步。

Native：macOS arm64 Darwin25.6、rustc/cargo1.99.0、Node26.0.0；隔离libheif1.23.1 prefix=/tmp/wardrobe-media-native/install，PKG_CONFIG=/opt/homebrew/opt/pkgconf/bin/pkg-config，PKG_CONFIG_PATH该prefix/lib/pkgconfig，DYLD_LIBRARY_PATH该prefix/lib。CARGO_INCREMENTAL=0、jobs2、DEV/TEST DEBUG0，独占root target。全部对象File.sync_all，同FS atomic rename；Unix额外staging目录sync。Windows普通File::open目录风险经源码审查用cfg处理，不静默吞文件IO错误，不声称Windows目录项crash-durability/native已实测。完整三平台/五CI gate仍归#35；新CI在runner启动前被账户billing/spending拒绝，未计为通过。

资源失败：一次rust-objcopy ENOSPC产生不可执行测试，测试未启动；保留captured excerpt（不是完整raw），仅删除本次坏产物重建，随后终态通过。清理19个确认终态rootcache项目test可执行，回收1947.7MiB，保留最新CLI/releases/rlib/rmeta/models/source/log；见cache-cleanup.json。初次GUI红缺入口超时，finally已闭context遮蔽主错误，临时钥随后由trusted CLI清除；最终cleanup用同一个test DB和stdin CLI。最终测试进程全部终态，无模型/GUI后台服务保留。
