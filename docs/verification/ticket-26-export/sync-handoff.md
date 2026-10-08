# #27 密文包复用边界

Application encryption.package.export/read 和 ADR0019 是独立于transport的白名单快照与认证文件包边界。#27应复用，不另行序列化整库。当前实际文件系统验证不等于远端服务、上传任务、冲突合并或发布完成。

事实：成员投影、clothing全生命周期事实、当前outfits及全部outfit_versions、wear全部修订与完整source_outfit、care_config/care全部修订与完整garment snapshot/care_asset_refs、insights_preferences。所有字段显式选择，配置身份/revision/时间保留。待洗计数、availability、库存分布、频率、组合等从事实重算，不存/不导出派生结果。默认未设置偏好没有伪造已存实体。

共享assets.sync_manifest授权当前采用WebP以及旧outfit的全部asset_ids/display_asset_id、wear_asset_refs、care_asset_refs中的曾采用展示图。它用于包生成及后续同步资产选择。只读display规范字节，核对SHA256、长度、WebP与4096边长；缺失损坏失败，不读取原图兜底。不要把job/CAS人物/试穿/未采用预览/本机缩略图加入资产白名单。

不可变随机目录和文件名，密文inventory绑定package/object/kind/hash/length/manifest hash。公开协议头携现有原始签名清单和space/epoch。当前收件状态必须本地验证并接受，read不擅自应用清单或导入事实。得知当前撤销后拒绝；未知撤销设备仍可能解读它拥有的旧包，不承诺全网离线撤销或删除旧下载。

当前手动单包容量明确有限：10000对象含一个snapshot、每个明文16MiB（blob含base64）、每个文件24MiB、对象密文合计256MiB。超限原子失败，不静默删图片或截事实。#27完整规格10k衣物/100k穿着需要分页、分片和适合网络流式传输的索引协议；必须沿用字段与资产授权规则。外部读取用File.take(cap+1)，不是stat后无界read。对象文件sync_all；Unix额外目录sync，Windows仅明确的文件flush与同FS atomic rename，原生目录项crash-durability仍未证明。

本票没有storage账号/云凭据、S3/WebDAV传输、自动调度、outbox/inbox消费、事实merge/冲突规则。后续应实现这些真实行为，并在当前可信manifest/epoch下验证接收，无权仅因能解开旧AEAD就接收旧设备数据。加密私钥/钱包/恢复材料、provider/client tokens、original/local_path/EXIF永不进入事实/图片包。用户主动录入notes等文本是事实，不能按猜测自动删除内容。
