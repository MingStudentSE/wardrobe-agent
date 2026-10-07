# AI 可操作的衣橱 CLI（Beta 交付设计）

关联任务：[完整外部 Agent 工作流 #33](https://github.com/MingStudentSE/wardrobe-agent/issues/33)。本文件描述必须交付的契约，不代表当前实现已经满足。

## Stage 1 已实现参数语法

类型化解析在打开任何数据库之前完成；语法错误一律返回 `invalid_arguments`，退出码 2，且不创建数据库、目录或 WAL。

- 元数据命令：`wardrobe-cli --help`、`--version`、`help <operation>`、`schema <operation>`（`help` 不带参数等同 `--help`）。元数据必须独立使用，仅可与全局 `--db` 同时出现；输出为单个版本化 JSON 信封 `{"schema_version":1,"result":…}`，退出码 0，无任何存储副作用。`help`/`schema` 的操作元数据来自 `wardrobe_core::operations()` 静态注册表；未知操作返回 `unsupported_operation`（退出码 2），不显示为可用能力。`--version`、帮助内容与 MCP `serverInfo.version` 均使用 Cargo 包版本。
- 全局参数：`--db <路径>` 恰好消耗其后一个参数作为值（即使该参数形如 `--help` 或 `mcp`，也是字面量路径）；`--db` 重复、缺值，`--trusted-local` 重复，未知旗标，多余位置参数，`--input-stdin` 与 `--client-stdin` 同时出现或缺少操作命令，均在打开数据库前以 `invalid_arguments` 拒绝。`--` 之后所有参数按字面量处理（命令槽与 JSON 输入槽）。
- 传输切换：仅当第一个位置参数就是 `mcp` 时进入 MCP JSON-RPC 传输；`--db mcp capabilities` 或把 `mcp` 用作输入参数都是普通 CLI 调用，绝不读取 MCP 标准输入。MCP 启动失败向 stderr 输出非secret诊断并以退出码 1 结束，stdout 保持纯 JSON-RPC 流。CLI 专属参数不得进入传输解析。
- 授权模式：`--trusted-local` 是本机人工维护通道（绕过客户端授权）；外部 AI 一律使用 `--client-stdin` 的 `{token,input}` scope 限定客户端路径；令牌与供应商密钥禁止进入命令行参数。`provider.credential.set` 与 `encryption.*` 仅接受标准输入（`secret_requires_stdin`），该检查同样先于任何数据库打开。
- 信封与退出码：CLI stdout 恰好一行可解析 JSON；成功 `{"schema_version":1,"result":…}`，拒绝 `{"schema_version":1,"error":{"code":…,"message":…}}` 退出码 2；存储失败（任何执行阶段 `storage_error`，含启动失败）退出码 1（MCP 传输除外，见上，其业务错误保持 JSON-RPC isError 语义）。stderr 仅输出非secret诊断。

## 安装与独立运行

macOS Beta 安装后提供稳定可执行命令 `wardrobe-cli` 和明确的 PATH 安装/验证方式；用户及 AI 不需要源码、Rust 工具链或正在运行的 GUI。`wardrobe-cli --version`、`wardrobe-cli --help` 和操作级帮助说明数据目录、授权、输入输出与可运行示例。卸载/升级与 GUI 共用数据保留策略。Windows/Linux 发行延后，CLI 的参数解析、协议与业务核心保持跨平台。

## 发现与调用

保留现有 `wardrobe-cli <operation> --client-stdin` 调用入口，stdin 接收 `{ "token": "连接令牌", "input": { ... } }`。令牌及供应商密钥不得作为命令行参数、输出、日志或普通设置中的明文保存。一次授权后允许在有效成员与 scope 内持续调用；每次请求检查到期、撤销、schema 和数据归属。授权或凭据变更走现有安全授权边界。

### scope 元数据（Stage 2，机器可读）

`wardrobe_core::operations()` 的每个描述符携带 `scope` 对象，由 `authorization::scope_metadata` 从与运行时执行相同的 `privileged()`/`is_global_operation()` 规则和精确授权分支派生（单一来源，不是第二套命令表）。稳定表示：

```json
"scope": {
  "access": "external | trusted-local-only",
  "primary": "<操作名>",            // limited 授权按此精确名匹配；full 授权含全部 external 操作
  "member": {"grant": "all-members | space-level | filtered | per-member_id"},
  "additional": [                    // 可选；组合/参数依赖授权条件
    {"when": {"reaction": ["actual_wore","changed"]}, "requires": ["wear.record"]},
    {"requires": ["feedback.record","recommendation.change"]},
    {"when_job_kind": "learning_recompute", "requires": ["learning.recompute"]},
    {"requires_original_job_scope": true, "job_field": "id"}
  ]
}
```

`trusted-local-only` 恰好覆盖 `client.*`、`provider.credential.*`、`encryption.*`：外部调用一律拒绝，`primary`/`member` 不参与判定（`primary_grant_required=false`）。`primary` 是 limited 授权按精确名匹配的操作名；`primary_grant_required=false` 表示该 primary 检查被豁免——`capabilities` 凭任意有效授权即可发现（`access=external` 但豁免）。`primary_grant_required=true` 时，limited 授权必须包含 `primary`（该 primary 精确名检查仅适用于 limited 模式；full 模式豁免 operations 检查）。`member.grant` 描述受限成员授权（`member_ids` 为非空数组，**含配合该数组的 full 模式**）下的执行语义，由操作的有效输入 schema 与授权豁免清单共同派生：`all-members`（`backup.*`/`data_directory.*`/`maintenance.*`，以及有效输入不含 member_id 的操作——如 `space.default`——受限授权在执行分支 `member_denied`）、`space-level`（`capabilities`、`provider.list`、`provider.validate`，无需 member_id）、`filtered`（`member.list` 按授权成员过滤）、`per-member_id`（输入 schema 声明 member_id：受限授权必须提供且仅能使用授权内成员）；仅 `member_ids: null` 的授权不受这些分支限制。组合条件 `additional` 是运行时执行的真实分支（反馈实际穿着/Try Another、学习重算任务、维护控制依赖原任务 scope），元数据不得放宽或虚构绕过。

### effect 元数据（Stage 2，机器可读）

描述符携带 `effect` 对象描述实际调度行为：`{"kind":"query|mutation|maintenance","network":"none|explicit-provider","local_files":true|false,"async":null|{"enqueue":…,"process":…,"status":…,"terminal_states":[…]}}`。`network:"explicit-provider"` 仅限确会联系所配置供应商的操作（如 provider.validate）；异步族必须写实际的任务登记/执行/状态跟进操作与真实终态集合，不得使用通用全任务模板。存在分支行为的操作用 `conditional_effects:[{"when":{…},"effect":{…}}]` 表达条件性变更（如 `assets.preview` variant=thumbnail 且缩略图文件缺失时同步重建本地缩略图），不得把条件变更笼统标为必然变更或必然只读。

能力发现返回实际注册且可执行的 operation、输入/输出 JSON schema、所需 scope、读写性质、示例与异步任务语义；发现列表与 CLI/MCP 实际调度共用注册来源，不维护容易漂移的第二套命令清单。未知能力不能显示为已实现。帮助与 schema 足以让 AI 自行构造请求和处理错误。

## 全量业务覆盖

通过共享 Core/Application API 操作全部 F01–F30 所要求的业务数据与动作：成员与分类、衣物/标签、照片导入和编辑/排序/主图/商品展示图采用、穿搭/Lookbook、穿着记录、洗护、统计、推荐/反馈/偏好学习/重置、配对/搭配图/试穿/Stylist、旅行与购买计划、定时推荐、设置、模型能力、同步后端/冲突/运维、备份/恢复及任务查询。CLI 不直接改 SQLite 或绕过领域校验。功能未完成时保持 ticket 开放，不能靠 CLI 占位命令宣称覆盖。

关闭内置 AI 时，外部 Agent 仍能读取上下文并写回标签、推荐、配对等结果；写入后 GUI 与 MCP 能读取同一持久数据。原图、中间图、试穿及个人图片保持本地；同步仅包含采用后的压缩商品展示图。

## 稳定输出与错误

stdout 仅输出可解析 JSON；诊断写 stderr，保证退出码与结构化错误一致。版本化响应包含稳定错误码和可操作说明，明确 unauthorized、scope 拒绝、schema/成员/ID 错误及业务冲突。列表支持公开 schema 描述的筛选、排序和分页方式；分页边界、空结果、非法参数的行为一致，不静默截断。异步生成、同步或恢复返回稳定任务 ID，并可查询状态、结果和失败信息；不得把提交成功当成任务完成。

## 验收边界

使用已确认的 Application API commands/queries 主边界、真实 SQLite 和临时资产目录，以及 CLI/MCP 的进程协议边界验收。实际安装环境中关闭 GUI，以可发现的命令完成读写、分页、错误、异步任务和授权撤销；随后 GUI/MCP 读回一致结果。完整授权与限定授权均验证，凭据不进入进程参数或日志。覆盖表逐项对应 F01–F30 实际 operation 与结果证据；真实 provider 的成功不能由 mock 代替。

### examples 元数据（Stage 2，机器可读）

已标注家族的描述符携带 `"examples":[{"request":{…},"prerequisites":["符号化前置指令",…]}]`。`request` 必须通过描述符 inputSchema 校验（公开契约测试结构断言 + 独立 jsonschema Draft 2020-12 审计），成员/自定义 ID 使用 `<符号占位>`；`prerequisites` 声明可执行前置、授权通道与付费/上传同意语义（如 provider.validate 每次调用可能消耗供应商额度且示例不自动执行；凭据仅经 stdin）。示例不得包含真实用户图片、密钥或令牌。
