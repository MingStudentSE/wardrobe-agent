# ADR 0034：持久计划、设备执行与通知提交边界

状态：macOS 已完成本机运行验收（2026-10-07，ticket-25 集成候选）；Windows/Linux 平台适配接口保留，真实平台运行验收延后。

计划是成员事实，包含工作日（ISO 1–7）、本地时刻、IANA 时区、场合、当天/次日目标、执行设备、启用状态、修订生效时间和补跑窗口。默认窗口120分钟，可显式修改为1–1440分钟。新增与编辑写事实、审计及outbox同一IMMEDIATE事务。修改只对未来生效，并取消该计划旧修订未结束任务。

本机设备ID、评估游标、重复执行安全receipt、系统权限及后台偏好不属于portable业务事实。空间+成员+计划+实际到期UTC构成执行身份。每次恢复只选择最近一个到期且落在窗口内的执行；较早窗口记missed区间，最近一周逐条保留。首次创建不得编造生效前错过。夏令时重复时刻选择较早UTC一次；不存在的春季时刻不执行，必须保留可解释记录。时区变更从修订生效时间开始，不补执行旧定义。

到期事务立即登记持久任务及receipt，业务worker与通知适配分离。成员限定客户端还须原始care.reminders/recommendation.generate scope；worker scope不能授予新业务权限。worker领取与结果发布检查计划修订、设备、暂停状态和lease。生成/通知不记录wear。AI子任务的成功、未知提交、失败、取消必须按真实终态传播，不把queued receipt报为推荐完成。

通知outbox先登记提交意图再调用OS，崩溃后unknown不盲重发。OS API接受只表示submitted，不保证用户看到了通知。拒绝权限、关闭系统提醒和适配器不可用均显式显示。后台驻留与开机启动独立且默认关闭；只有可信本机明确操作可更改。关机或所有进程退出无法执行，到恢复时遵循补跑规则。

当前原生通知适配实际使用 `notify-rust`：macOS 设置应用标识，Windows 设置应用 ID，Linux 设置应用名；不是 Tauri notification 插件。系统接口成功仅记录 submitted，接口错误记录 unknown，不盲重发。当前状态接口明确返回权限由操作系统管理且不可查询，尚不能证明拒绝权限时的完整交互已验收。

登录启动实际使用 Tauri autostart 插件（https://v2.tauri.app/plugin/autostart/），macOS 使用 LaunchAgent；默认不调用 enable。macOS 原生编译检查和浏览器 GUI 测试已通过，但三平台安装后的通知、后台驻留、登录启动及系统权限交互仍须实际运行验收，不能用这些检查替代。

2026-10-07 macOS 本机运行验收（真实进程、隔离数据目录、notify-rust 真实系统通道；证据见外盘 `results/continue-20261007/ticket25/native-run/`）：到期执行→worker 成功→通知真实提交（submitted）达成；持续 5 秒循环与手工重 tick 均不重复执行或重复通知；`background_enabled=false` 时窗口隐藏、到期不执行，重新启用后补跑一次；真实通道拒绝路径实际发生——set_application 失败按 failed 记录「尚未提交，请核对安装与系统设置」，显式 `schedule.notification.retry` 允许 failed 重试并再次如实记录失败；进程退出跨越到期时刻后重启，默认 120 分钟窗口内补跑一次并通知，`catchup_minutes=1` 的窄窗计划按配置标 missed 且不通知；创建晚于到期时刻的计划不追溯执行。macOS 上系统横幅是否被用户看到仍不可查询（submitted 只表示系统接口接受）；托盘点击退出、系统设置中权限拒绝/重新开启的完整人工交互、真实睡眠唤醒及登录启动 LaunchAgent 状态仍属人工验收项。Windows/Linux 的 notify-rust/autostart 适配接口保留，未做真机验证。

schema21预留本模块，按实际表存在修复且user_version取最大；20留给独立learning实现。portable纳入计划与业务执行历史，排除本机表与通知权限。恢复旧备份不能退回本机去重安全状态；目录迁移保留它们。
