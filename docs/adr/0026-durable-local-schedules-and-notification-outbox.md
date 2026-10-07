# ADR 0026：持久计划、设备执行与通知提交边界

状态：实施中，#25 尚未验收。

计划是成员事实，包含工作日（ISO 1–7）、本地时刻、IANA 时区、场合、当天/次日目标、执行设备、启用状态、修订生效时间和补跑窗口。默认窗口120分钟，可显式修改为1–1440分钟。新增与编辑写事实、审计及outbox同一IMMEDIATE事务。修改只对未来生效，并取消该计划旧修订未结束任务。

本机设备ID、评估游标、重复执行安全receipt、系统权限及后台偏好不属于portable业务事实。空间+成员+计划+实际到期UTC构成执行身份。每次恢复只选择最近一个到期且落在窗口内的执行；较早窗口记missed区间，最近一周逐条保留。首次创建不得编造生效前错过。夏令时重复时刻选择较早UTC一次；不存在的春季时刻不执行，必须保留可解释记录。时区变更从修订生效时间开始，不补执行旧定义。

到期事务立即登记持久任务及receipt，业务worker与通知适配分离。成员限定客户端还须原始care.reminders/recommendation.generate scope；worker scope不能授予新业务权限。worker领取与结果发布检查计划修订、设备、暂停状态和lease。生成/通知不记录wear。AI子任务的成功、未知提交、失败、取消必须按真实终态传播，不把queued receipt报为推荐完成。

通知outbox先登记提交意图再调用OS，崩溃后unknown不盲重发。OS API接受只表示submitted，不保证用户看到了通知。拒绝权限、关闭系统提醒和适配器不可用均显式显示。后台驻留与开机启动独立且默认关闭；只有可信本机明确操作可更改。关机或所有进程退出无法执行，到恢复时遵循补跑规则。

原生接口使用Tauri官方插件：
- https://v2.tauri.app/plugin/notification/ （Windows必须安装后的应用，浏览器测试不证明通知显示。）
- https://v2.tauri.app/plugin/autostart/ （macOS LaunchAgent、Windows及Linux启动机制；默认不调用enable。）

schema21预留本模块，按实际表存在修复且user_version取最大；20留给独立learning实现。portable纳入计划与业务执行历史，排除本机表与通知权限。恢复旧备份不能退回本机去重安全状态；目录迁移保留它们。
