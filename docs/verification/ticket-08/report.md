# #8 手工搭配 / Lookbook 验证

2026-10-07；独立 worktree 从 d0b22ef 开始，Application API（真实临时SQLite）、CLI/MCP（真实进程）和GUI（真实浏览器→CLI→SQLite）。未读取个人衣物照片、未使用供应商key。GUI沿用Atelier系统，仅新增Lookbook组件、导航链接与共享样式；无删除旧组件，关键动作无hover/拖拽依赖。

红→绿证据：首个保存/版本公开API测试报 unsupported_operation；实现后通过。GUI 首测找不到“搭配”导航，新增实际入口后保存/重启、层次角色、收藏、克隆、回收站恢复通过。独立资产引用测试最初在detach后读到 local_only，改为统计不可变outfit_versions引用后变为workspace，历史拼贴仍使用该展示图。编辑未声明版本测试起初错误地成功；明确 expected_version 后拒绝缺失版本，过时编辑不覆盖现有模板。

5个核心Lookbook外部测试覆盖全部11种类别、空/重复/不存在/错误成员拒绝且不留下模板、独立角色层次、真实WebP像素、缺图回退、收藏/克隆/删除恢复、重启及旧版本稳定；Clothing get与history确认保存模板不修改单品事实/历史。1个真实CLI/MCP用例比较完整持久版本 JSON。当前还没有WearEvent模块，不宣称已完成真实穿着的行为测试；#9 将负责显式今天穿/改穿。

运行：cargo test --workspace（59通过、1个已有OS凭据测试明确ignored）；cargo clippy --workspace --all-targets -- -D warnings；cargo fmt --all。Native环境为独立libheif/libde265 prefix；这不是跨平台安装包验收声明。

GUI测试验证320/375/414/768/1024/1280/1440无横向溢出，截图 lookbook-320.png / lookbook-1280.png，已实际查看320截图。首次全套运行18通过、Lookbook用例失败：它引用的“分页单品1”已被前面的管理测试改名；改为未被其他用例修改的“分页单品23”。添加读取状态的红→绿验证后，全套20项GUI测试通过；明确加载时不显示空结果且暂时禁用编辑。

真实公开样本拼贴：public-sample-collage.webp 为实际CLI导入、采用两张展示图后渲染（800×480，missing_images=0）。已实际查看：两个完整物件保留比例，展示区未裁切，背景正常；这是历史藏品图像拼贴，未宣称服装搭配推荐或AI生图成功。来源 Cleveland Museum CC0：[Bridal Robe (Hwarot)](https://clevelandart.org/art/1918.552)、[Hat (Kalyeem)](https://clevelandart.org/art/1998.29)，源许可与hash见 source-manifest.json；不复制个人图片。

完整Beta后续门槛保留：真实AI flat-lay、WearEvent今天穿/改穿、同步冲突与最终三平台安装/原生GUI交由后续票，不缩减父规格。
