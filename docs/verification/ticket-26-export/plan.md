# #26 加密文件包补充计划

确认测试边界：公共 Application +真实临时SQLite/文件夹/OS凭据；真实CLI/MCP授权；GUI→本源CLI Application。沿用父任务已确认seams，不mock领域读写。仅受信任本机 encryption.package.export/read，CLI强制stdin，MCP不列出；无远端同步调度或事实合并。

第一slice：配对两安装，真实衣物及采用展示图由应用一致快照白名单打包，kind分离AEAD；独立目录只含认证密文和必要公开协议，第二安装验证并读出（不导入）事实/展示图。先红再实现。

后续slice：当前和历史搭配/穿着引用、未采用/原图/本机私有状态排除；文件/hash/AEAD/清单/当前manifest/epoch/撤销拒绝；源损坏不原图兜底，发布原子且不覆盖；GUI明确选择文件夹，CLI/MCP权限和实际两安装复验。不可用OS钥或当前可信状态缺失必须失败。离线设备未得知撤销不能宣称全网撤销。

包生成器供#27复用。实际文件夹包验收不声称S3/WebDAV、inbox应用、冲突合并或#35全平台完成。无需新数据库表。
