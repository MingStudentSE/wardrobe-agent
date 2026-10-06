# ADR 0003：单品事实编辑与可恢复生命周期

状态：已采用（2026-10-07），父规格 #1，垂直切片 #4。

Application API 的 `clothing.update` 为局部事实补丁；只改调用者提供的可编辑字段，保留未提供事实、图片关联和未来引用字段。字段使用 docs/05 的 canonical `sub_category`、`color`、`pattern`、`material`、`style`、`occasion`。`capabilities.taxonomy` 公布 version 1 小类与所属大类、稳定颜色 ID；其他颜色使用 `other` 与可选 `color_description`。自由文本多值按去空白和小写键去重，保留首个展示文本。未知可选文本/购买事实为 null，未设多值为空数组。金额为 JavaScript 安全范围内的非负最小单位整数，ISO 币种指数来自同一 Rust 货币表。

生命周期与收藏独立。`clothing.archive`/`activate` 显式设置可用状态；`clothing.delete` 保存整个事实文档与 `status_before_delete`、`deleted_at` tombstone，不硬删。`restore` 显式恢复到删除前 active/archived 状态，保留 ID、创建时间、收藏和资产引用。已删除单品拒绝普通编辑与归档，避免迟到写入复活。领域写入、audit 快照与 outbox 保持同一事务；旧 audit 快照不被后续编辑改写。`clothing.history` 先检查成员与单品范围，再分页返回已有事实变更快照；它不是实际穿着记录。

列表默认只显示 active，支持 archived/deleted 查询。搜索与多个筛选取交集，排序始终以 ID 打破相等项，游标为当前排序中最后一条单品 ID；不属于筛选结果的游标返回 invalid_input。价格排序按币种分组，未知金额最后，不虚构汇率。新增 JSON 事实和索引在现有迁移事务中幂等初始化，不单独提升数据库版本。

GUI 删除入口仅位于详情编辑区，提供确认和回收站恢复；复用既有 Atelier tokens。CLI 与 MCP 直接使用相同 API，不依赖 GUI。真实 SQLite 用例验证字段往返、过滤排序分页、事实快照和软删除恢复；浏览器通过真实 CLI 桥验证关键流程与指定七个宽度。
