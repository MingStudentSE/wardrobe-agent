# ADR 0017：按有效时间线计算洗护与可用性

状态：采用（#10，待独立验收），2026-10-07。

每个成员的真实单品身份拥有独立洗护配置。wash_after_wears 默认为 null，配置只能是正整数或 null；配置 revision 使用 CAS，操作标识保存完整意图收据。待洗不改变 active/archived/deleted 生命周期。照片相同不表示同一件衣物，多个模板引用同一件衣物则共用它的洗护投影。

care.record 保存 wash/dry_clean/maintenance；care.mark_dirty 保存标记或解除手动待洗的时间事实。所有事件是 UUIDv7 追加根与修订，care.correct/revoke 用 expected_revision CAS，历史、快照和资产引用永久保留。回收站单品拒绝新增，已有快照允许更正时间与类型、撤销。事件、收据、audit、outbox 同事务。DB13 依据真实表幂等初始化，保留 DB12 recognition 与 DB10 encryption 表及最高版本。

计算使用 #9 effective_wears 最新非撤销根，再合并最新非撤销 CareEvent，按 UTC occurred_at 升序；同一瞬间先穿着后洗护，洗护相同瞬间按 root_event_id。wash/dry_clean 把此前累计次数归零，并解除此前手动待洗；maintenance 不重置次数或手动标记。之后的手动标记仍保留，解除手动标记不抵消阈值到期。补记、更正、撤销与阈值变化均重新投影，无可漂移的可变计数器。

复用穿着的严格 IANA 时区与日期校验，拒绝未来日期及 DST fold/gap。只知日期时当地中午是排序锚点，不是已知实际时刻；输出 chronology_uncertain 和确定排序规则，UI 说明同日先后不确定并提供时间更正。

care.status / availability 给后续推荐提供 available 与稳定 exclusion_reasons（manual_dirty、threshold_reached、archived、deleted）。待洗不阻止用户如实记录实际穿着。care.dirty_list / reminders 给出需洗护条目、累计次数、阈值与最近清洗，notification_scheduled=false；自动通知调度由后续任务交付。本模块不借此缩减 Beta 总体功能。

GUI、CLI、MCP 共用 Application 和 operation/member scope，没有 GUI 独有事实写入。
