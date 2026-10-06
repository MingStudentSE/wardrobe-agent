# ADR 0007：本机客户端授权边界

状态：已采用（2026-10-07），#12 授权基础；#27 / #33 在此基础扩展同步及全量能力。

可信 GUI 调用 Application.execute，登记、列出、撤销本机客户端。外部调用必须用 Application.execute_as_client(token, operation, input)，先检查数据库版本，再查 token hash、撤销/到期、操作scope与成员范围，最后调用同一业务 Application。后续适配器不可直接调用 execute 绕过外部授权。

DB5 创建本机 clients 表，不写 outbox，不进入设备同步。提升全局版本使旧的、没有授权层的CLI无法继续打开新版数据库。迁移在已有IMMEDIATE事务内创建表，检查实际表存在确保并行迁移幂等。授权登记/撤销也在共用 begin_write 中提交。其他模块不能把DB降回4。

令牌由OS随机源产生32字节（256bit），仅登记返回一次；数据库仅保存SHA256 hash及非敏感授权元数据。list/revoke/错误输出无token或hash。token不作CLI参数，不写普通配置/日志。令牌只能用于其登记数据库；不同空间需分别登记。到期时间明确RFC3339或null，成员范围明确UUID数组或null，不允许缺字段默认为全部成员。损坏的授权文档返回storage_error，不扩大范围。

scope的稳定名称是实际Application operation名。limited仅允许所选已实现操作，未来新增操作须重新授权；full允许当前及未来已经实现的业务操作，不对每次调用再弹权限框。未知operation默认拒绝。client.*和provider.credential.*只属于可信本机设置，任何外部scope都不能自授权限或管理系统密钥。provider.validate可在明确scope内调用已配置模型，会使用供应商账户额度。provider.save不可间接清理已有bearer模型凭据：端点/认证绑定变化在锁内检查，需可信本机设置处理；外部仍可编辑模型名称等不涉及凭据变化的配置。

成员范围null表示明确授权全部成员（包含以后新增成员）；数组范围经过共用成员解析器canonical化。限定成员的请求不能访问其他成员；member.list仅返回授权成员，未授权selected_member_id变为null。缺少member_id的空间级业务默认拒绝，备份、恢复、登记新成员等需要全部成员范围。无成员事实的capabilities/provider.list/provider.validate是明确例外，不让未来未知全局工具自动获得例外。

CLI默认外部模式，无token业务返回unauthorized。`operation --client-stdin`从标准输入读取 `{ "token": "…", "input": { … } }`。本机人类维护显式使用 `--trusted-local`，等同于拥有数据目录的本机维护权限，不用来连接外部客户端。GUI开发专用loopback桥显式用该flag，并拒绝跨Origin请求；生产Tauri直接可信调用，不运行开发HTTP桥。

MCP通过initialize.params.authorization_token或tools/call.params.authorization_token接收stdin令牌。初始化一次后同连接复用；每次业务仍从数据库检查撤销/到期。tools/list和capabilities仅列可用scope，不把权限变更当为会员限制。MCP即使收到--trusted-local也不绕过令牌层。

这不是同一OS用户之间的文件沙箱：持有本机维护权限的人可直接操作SQLite或主动使用可信flag。授权层保护按登记协议连接的外部客户端，不声称阻止同一OS用户的主动文件/维护入口绕过。撤销拒绝之后的新请求，不声称取消已经通过检查并开始执行的在途请求。

#26/#27需排除clients、本机令牌、供应商凭据及其他本机授权状态的跨设备同步/普通导出；恢复后仍由可信本机用户处理客户端授权。业务scope默认不能用于登记额外客户端。
