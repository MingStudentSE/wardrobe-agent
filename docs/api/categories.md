# 衣橱分类管理

GUI 分类栏的“管理分类”和 CLI / MCP 共用以下 Application API。分类按 `member_id` 保存。

| Operation | 输入 | 输出 |
| --- | --- | --- |
| `category.list` | `member_id` | `items: [{id, name, count}]` |
| `category.create` | `member_id`, `name`（1–40 字，不能重名） | `{id, name, count}` |
| `category.delete` | `member_id`, `id` | `{deleted, moved}` |

新增分类返回稳定 ID，`clothing.create` / `clothing.update` 的 `category` 使用这个 ID。默认分类仍使用 `top` 等旧 ID。`capabilities.categories` 是内置分类列表；当前衣橱应读取 `category.list`。

删除分类原子地把所有引用该分类的单品（含归档、回收站）转到 `other`，保留衣物和穿搭快照，记入历史与 outbox。GUI 先确认删除。`other` 是兜底分类，不能删除。分类定义包含在本地备份中；数据库版本 21，兼容恢复旧版本备份。

CLI 调用示例（可信本机入口）：

```sh
wardrobe-cli --trusted-local category.list '{"member_id":"default"}'
wardrobe-cli --trusted-local category.create '{"member_id":"default","name":"汉服"}'
wardrobe-cli --trusted-local category.delete '{"member_id":"default","id":"返回的分类ID"}'
```

MCP 可通过 `category.list`、`category.create`、`category.delete` 工具发现并调用相同操作。
