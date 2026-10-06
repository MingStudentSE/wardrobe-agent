# #26 应用密文文件包补充验证

当前为可审查实现，等待 care/insights 最新集成后的最终联合检查；不是最终验收结论。

Application 自己从真实数据库一致快照选择事实与采用展示图，写入独立文件夹的认证密文包。真实两 CLI 安装已配对，第二安装验证／解密事实与展示字节，不导入衣橱。外部三文件逐字节扫描未见事实名称／备注、源文件名、原图字节或本机路径；结果见 actual-cli.json 和 reproduce_cli.py。此证据是实际文件系统边界，不是S3/WebDAV／远端调度／事实合并。

实际红绿：第一条公共 Application 回归在缺少 package.export 时 unsupported_operation；另一个真实旧搭配非主图回归在修复前 expected2 got1。共享 assets.sync_manifest 现在保留 immutable outfit 快照 asset_ids/display_asset_id 与 wear_asset_refs 所授权的展示图，未放开未采用图片或原图。

当前 focused 终态13 passed：9媒体资产公共行为，4包行为（3真实OS临时钥场景+1full客户端拒绝）。配对场景还覆盖损坏文件、签名篡改、源展示图损坏无原图回退、原子不发布、当前epoch与已知撤销；1700真实单品×10000备注形成超限事实，失败前外部目录0文件、本地事实保留。截图/GUI通过1真实OS测试，七档宽度320–1440无溢出，实际查看320像素图。初次GUI红缺入口发生超时，finally闭合context遮蔽主错误，临时钥已另外通过Application CLI清除；后续cleanup使用相同test DB和stdin CLI保证解除。

包读取校验当前 raw 签名清单与空间／epoch／revision，未知撤销的离线设备仍能读已持有旧密钥的旧包；获取并应用撤销清单后必须拒绝。不声明远程擦除。CLI stdin与full客户端拒绝、MCP授权tools/list排除encryption已实际运行。GUI native目录picker已实现并构建，自动回归使用可触控文本路径；没有冒充原生Tauri picker弹窗实测。

资源：一次新增测试产物在rust-objcopy遇ENOSPC后不可执行，测试未启动；删除本次损坏可执行后重建终态通过。日志仅保存捕获的失败excerpt，非完整raw；没有把基础设施失败计入产品失败。原生依赖prefix、debug0、j2、incremental0，独占root target，无全局安装。

容量与后续协议见ADR0019：单手动包不截断，超限明确失败。#27应复用白名单和包协议，增加分页／分片、运输、outbox/inbox与冲突处理，覆盖完整规模。#35所有原生跨平台发布检查、账户阻断的新CI及实际云API gates继续保留。没有push／关闭Issue／新增表或数据库版本。
