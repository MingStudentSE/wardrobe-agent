# ADR0018：衣橱洞察、统计分母与成员偏好

状态：已采用，#11 / F13，2026-10-07；本机行为验收与各平台发布验收分别记录。

`insights.summary` 是同一SQLite读取快照中的只读派生查询，由GUI/CLI/MCP共享Application。输入member_id、as_of_date、IANA timezone；as_of_date不能晚于该时区今天。库存采用**当前active+archived**，deleted数量另列，不进入类别/颜色分母；历史截止日期只筛选穿着时刻，不声称能还原过去库存或过去修订知识。

类别每件一次。颜色为多值，每件每种颜色最多一次，空颜色计入unknown/null；每桶percent=桶内衣物数/当前库存数×100，多颜色桶总和可超过100%。零库存返回空桶，避免无定义比率，不猜颜色。

频率从`effective_wears`的最新非撤销事实根计算，按occurred_at转换查询时区后的当地日期≤截止日期筛选；未穿库存显示0次/0天。wearCount按有效事件，wearDays按事件保存的当地日期去重，不能改成UTC创建日。已删除衣物的有效历史仍统计，并以最新有效历史快照名标记“不在库存”。当前从未穿过与久未穿只列当前库存。

最近穿着日将最后有效occurred_at换算查询时区，与截止日期作自然日差。days_since_worn≥成员stale_days为久未穿；默认90，89不入、90/91入。从未穿过单列，不把缺记录当作90天前穿过。date精度事件的本地中午仅是原WearEvent已声明的锚点，不能冒充已知分钟。更正/撤销实时改变投影，不存累加计数。

组合为同次穿着完整衣物ID的无序集合，相同衣物的不同顺序/来源模板/角色不拆成不同组合，单件也可组成一个集合。每组次数/当地天数分别统计；保存搭配、推荐、AI搭配图或试穿结果不产生组合或穿着统计。

“可配置”须跨重启保存。DB14只新增成员FK的`insights_preferences`，保存UUIDv7、schema_version、member_id、stale_days、revision、created_at、updated_at文档；未设置时查询返回默认90及revision0，id/时间为null而非伪造已存实体。`insights.configure`要求expected_revision，SQLite IMMEDIATE事务内CAS，保存文档、audit、outbox同事务；null显式重置90并增加修订，不删除身份/创建时间。它是成员业务偏好，不是仅本机显示开关；未来#27同步白名单需包含该实体并按修订/冲突规则处理，禁止同步本机路径/令牌/私钥。此票未实现传输层，不声称已同步。

初始化按实际表执行，最高版本取max，不能因DB14已经存在跳过缺表修复或降低recognition12/care13。派生统计不入持久表/audit/outbox，不添加不必要索引或同步冗余计数。外部limited客户端只获insights.summary时不能configure，成员授权仍是既有canonical UUID规则。

GUI复用Atelier组件范围，不改宏观布局；文字+meter显示同一API分布，不仅靠颜色区分。日期/时区与阈值是显式控件，保存偏好后重载仍可读，默认重置可达。更正后用户明确重新计算读最新状态，结果标注已计算截止日期/时区，避免把旧图当实时结果。
