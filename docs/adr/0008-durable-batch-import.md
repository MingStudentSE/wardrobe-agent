# ADR 0008：本机持久批量草稿与 worker lease

状态：采用；对应 #6 / F02，2026-10-07。

批次最多100个文件，默认每文件一草稿，一张照片不会按内容自动拆成多实体。创建响应前把可读文件原始字节复制到现有 local-only CAS；记录不可读、大小超限错误而不阻断其他文件。重复 member_id + operation_key + 同文件列表返回已有批次，改变输入拒绝。原始源路径和任务输入仅存在本机表，不进入 audit/outbox 或同步清单。

`imports.create/get/list/process/group/update/commit` 贯通 Application API、GUI、CLI/MCP；批次ID与真实逐文件任务ID分开，响应 `job_ids` / `item_ids` 可用于 `jobs.get/cancel/retry`，不把批次ID伪装成 worker task。每次 process 只处理一项，GUI逐项驱动并读取实际持久状态，CLI/MCP调用方重复 process 或指定 item_id。默认300秒 lease，处理期间每500ms GUI查询当前批次；关闭所有客户端不会继续计算，重新打开能读到队列，过期运行任务可接管，未过期显示正在处理并可取消后显式重试。

jobs 的可信 Rust worker seam为 enqueue/claim/finish；远程客户端仅有经授权的业务操作，不暴露任意 enqueue、claim、finish。IMMEDIATE 事务保证单worker claim；完成必须匹配未到期 lease token，取消使旧token失效。未知能力继续默认拒绝；每个新操作须显式 operation grant + member范围。通用网络任务 error.retryable=true 才允许最多3次自动尝试，指数延时或 Retry-After 毫秒值限定1–300秒；永久错误只允许明确人工重试。当前批量文件解码为纯本地处理，所有本地解码/格式/大小错误不自动循环；无供应商调用或额度消耗。

每草稿允许最多20项图像，归并只接受完整未入库草稿，不隐式删除单品。草稿事实复用 Clothing 验证器，未知名称默认“未命名单品”、category默认other，update按patch保留已有确认事实。真正入库前所有图像任务须waiting_input。单品、采用的资产、草稿到clothing映射、任务完成状态、commit key、audit/outbox在同一事务里提交；同一草稿同 key/输入重试返回已有单品，竞争提交同样只产生一单品一历史。资产ID在任务创建时固定，崩溃后重复处理不会新建重复资产；CAS字节复用不代表去重单品。

取消保留原图和preview资产，仅影响未入库任务。取消正在进行的 native解码不强杀线程，结果不再写回任务或采用；已完成单品不受取消影响。归并/任务本身仍为本机资料，不同步设备路径或lease。DB6迁移始终检查实际表，最终user_version取当前支持/已有版本最大值；并行功能合并不能因全局schema数字更高而跳过模块表创建。
