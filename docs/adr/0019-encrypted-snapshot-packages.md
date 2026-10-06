# ADR 0019：应用白名单快照与独立密文文件包

状态：采用。关联：#26、ADR0012、ADR0010、ADR0011；#27传输后续复用。

#26 的通用 encrypt/decrypt 不能证明业务数据和原图排除。Application 现在提供受信任本机专用 encryption.package.export/read；CLI 使用标准输入，full 外部客户端与 MCP 均不能获得此管理权限。GUI 是同一 Application 边界。无需新增表或数据库版本。

导出用 IMMEDIATE SQLite 事务读取同一状态的成员、当前单品／搭配、全部搭配版本与穿着修订，字段逐项白名单，source_outfit 保留完整事实快照、角色、层次与图片引用。成员使用既有 member.list 投影。导出不包含任务、供应商、客户端凭据、钱包、恢复材料、原图、缩略图、本机路径及图片元数据。用户主动录入的 notes 等文本属于事实，不能按文本语义自动删改。

图片授权集中于 assets.sync_manifest：当前 adopted/workspace 产品展示图以及 immutable outfit 快照 asset_ids/display_asset_id、wear_asset_refs 所授权的曾采用展示图。只有 product_display/WebP 可以导出。读取实际展示字节、SHA256、长度及图像解码；缺失损坏必须失败，绝不回退原图。图片 ID 和内容 hash 在密文索引／对象中，不作为公开文件名。

包目录是随机不可变身份，只含 package.json 和随机名认证密文对象。公开头包含协议／空间／epoch 与现有原始签名设备清单；清单中的公开设备身份属于协议元数据。AEAD 索引绑定该清单 hash、sender、package ID、对象名字／用途／长度／密文 hash；快照与 blob 使用既有 HKDF 空间／epoch／用途派生及 XChaCha20-Poly1305。对象内再次绑定 package/object ID。所有文件同步后原子重命名 staging，不覆盖已有包，失败仅清理本次 staging。原图字节和路径不会进索引或对象。

读取先以固定 owner_public 对 raw manifest 严格验签，并要求与本机已接受的当前 manifest/revision/epoch 完全相同；不自动应用外来清单、不更新事实。接收者必须已确认且未被本机已知清单撤销，发送者必须在当前清单内。验证 inventory/hash/长度/AEAD/对象身份、WebP 格式、尺寸和分配限制后返回 inspection 结果。新清单需先用 device.apply 显式接受。旧包仍是历史密文；得知新 epoch 的设备不能把它当作当前可接收包。未知撤销的离线设备仍可能解读它已持有的旧密钥／旧包，不能声称远程擦除或全局离线撤销。

当前手动包的确定容量：最多10000对象（包含一个快照）、对象明文最多16MiB（blob 包括 base64 和元数据）、单个外部文件最多24MiB、密文对象合计最多256MiB。各事实集合读行累计也限制16MiB；组合快照最终执行同一明文限制。超限在发布前失败、不截断，不承诺此单包代替完整衣橱大规模同步。#27必须分页／分片覆盖规格10k衣物与100k穿着，在同一白名单 serializer/asset manifest 上实现transport、outbox、inbox、冲突合并；本票没有S3/WebDAV上传器。

实际验收为两个新安装、真实OS临时钥、真实衣橱事实／规范展示图，写入独立文件系统，再由配对安装验证解密并检查无原图／事实明文字节。该证据不等同互联网云存储传输、事实合并或三平台 Beta 发布。
