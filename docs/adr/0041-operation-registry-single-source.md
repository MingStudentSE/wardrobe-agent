# ADR 0041：操作注册表单一数据源与公开契约冻结

状态：已采用（2026-10-11；注册表由 #70 增量 1–4 实施，本 ADR 为集成 code-review 补记的决策记录）。

## 背景

#70 之前，公开操作的元数据分散在至少四处可以互相漂移的平行表：分发器的 match 臂、lib.rs 手抄字段白名单、描述符 JSON 里的 effect、授权 scope 元数据。新增操作要记得改每一处，漏一处只在运行期暴露（分发不到、或字段校验与公开 schema 不一致）。#70 用一个注册表（`registry.rs`，一个操作一行：名称、effect、分发处理器）收口，字段白名单改从操作自己的 `inputSchema.properties` 派生。审查要求把这一收口的决策、派生关系与对外行为变化记入 ADR。

## 决策

1. **注册表是公开操作的唯一数据源**：`registry.rs::declarations()` 一个操作一行，声明 (名称, effect, 分发处理器)。运行时按名索引由静态声明一次性构建（`declaration_index`），渲染 effect 时只读索引、绝不回调 `operations()`，避免 OnceLock 重入。
2. **派生而非抄写**：分发处理器、分发前字段白名单（= 公开 `inputSchema.properties` 的键集）、描述符 JSON 的 effect、授权 scope 元数据都从同一注册数据导出。新增操作只登记一行；「拒绝未知输入」成为全操作一致的不变量，分发前统一执行。
3. **冻结快照防漂移**：`tests/fixtures/capabilities-snapshot.json` 逐字冻结全部公开操作的描述符 JSON（name/description/inputSchema/outputSchema/effect/examples/scope），`operation_registry::public_operation_contracts_are_frozen` 语义对照；有意变更契约必须重新生成快照并在工单显式记录。
4. **行为变化显式记录**（#70 起，含集成审查收口）：
   - `category.*` 等此前从不校验未知字段的操作开始拒绝（`invalid_input`）；
   - 未知字段拒绝文案统一为「操作包含不支持的字段」。此前 `images.*`（「图片操作包含不支持字段」）、`care.*`（「洗护操作包含不支持的字段」）等族各有一份专属文案，注册表白名单在分发前统一拒绝后它们不可达；为避免维护按族文案的第二套映射，选择保留统一文案，由 `operation_registry` 测试对 images、care 两个代表族逐字钉住。错误码 `invalid_input` 不变。
   - `jobs.list` 的输入面收回到仅 `member_id`：#70 派生白名单曾因描述符 properties 声明了无语义的 `id` 而放宽接受（实现只读 `member_id`），集成审查判定该放宽超出生明例外，2026-10-11 将 `jobs.list` 的 `inputSchema.properties` 收回到 `member_id`，重新拒绝 `id`；`jobs.get/cancel/retry` 的 `id` 字段有真实语义，不受影响。
   - 未注册的前缀误用（如 `care.bogus`）统一落 `unsupported_operation`（错误码不变）。
5. **`sync.*`/`schedule.*` 家族公开契约本就未声明 effect，注册行保持 `None`**；schema_version 注入沿用既有前缀规则（`backup.*`/`data_directory.*`/`schedule.*`），谓词由测试钉住，新增操作须有意识地扩展而不是悄悄依赖旧行为。

## 后果

- effect、白名单、分发、scope 之间的漂移从「运行期事故」变为「快照 diff 评审时可见」。
- 家族分发器（`family_router`）只保留注册表外的前缀误用回落语义；各家族模块内的手抄字段校验在注册操作路径上不可达，后续清理另行立项。
- 契约变更的成本前移到评审：任何 inputSchema/outputSchema 改动都会体现为快照 diff。

## 测试证据

- `crates/wardrobe-core/tests/operation_registry.rs`：快照冻结、未知字段拒绝（含 category/images/care 族与统一文案逐字断言）、`jobs.list` 拒绝 `id`、schema_version 注入等价、前缀误用回落。
- `crates/wardrobe-core/tests/registry_contracts.rs`：输出 schema 校验器自测、全操作示例有效性、派生元数据抽样。
- `crates/wardrobe-core/src/registry.rs` 单元测试：注入旗标与旧前缀规则逐行等价、每个注册行必有分发处理器。
