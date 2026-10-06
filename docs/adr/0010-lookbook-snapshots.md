# ADR 0010：手工 Lookbook 与不可变搭配版本

状态：采用（#8），2026-10-07。

搭配是模板，不是真实穿着。outfit.create/update/favorite/clone/delete/restore 仅写 outfits、关联项、版本、audit 和 outbox；不调用穿着领域或改动单品事实。后续“今天穿”“改穿”须显式创建/更正 WearEvent，不能复用保存搭配作为穿着动作。

数据库8通过实际表结构幂等初始化 outfits、outfit_items 和 outfit_versions，使用同一 IMMEDIATE 迁移事务。outfit_items 的位置、角色和层次为关系字段；真实 Clothing 外键、组合内去重、成员归属在写事务内共同检查。角色独立于类别，所有衣物类别均可加入，至少一件；最多100件以限制本地拼贴资源。角色1–40字符、层次-100到100整数、名字1–120字符。回收站单品不能加入新结构，归档单品可作为已有事实加入。

每次搭配写入产生不可变版本快照：Clothing 当时事实与 display_asset_id、顺序/角色/层次随模板版本保存，后续修改单品/模板不会重写旧版本。克隆新UUID并复制源快照，是独立模板；编辑需 expected_version，过时版本明确返回 version_conflict。删除为可恢复状态，不清除版本。未来 WearEvent 可复制 outfit.version 的精确结构，不动态追溯现有单品。

本地拼贴从保存版本引用的规范化展示图解码，使用稳定层次排序（同层按位置），单件一列、多件两列；保留比例、不裁剪，缺图绘制占位边框且返回 missing_images。产物 WebP、role=outfit、sync_scope=local_only，只返回本地预览，不上传或使用原图/网络/key。渲染与模板保存分离，图片丢失不丢模板。assets.detach 计入所有 outfit_versions 的展示图引用，避免仍被版本引用的已采用图片被降为未采用预览；抠图/旋转/替换应使用新的资产身份保留旧 display。

本票交付 F08、F15 的离线拼贴及 F27 的模板管理切片。F15真实AI flat-lay、F27今天穿/改穿由后续依赖票完成，本文不将这些完整功能判完成。
