# #3 独立成员档案验证

2026-10-07，基于 integration 22ebdd7，真实 SQLite/Application、CLI/stdin MCP、Chrome GUI。成员 API 为 member.create/update/list/select；没有协作、邀请、共享角色。

TDD 红绿证据：
- 首个 profiles_survive_restart 测试：旧 name=我的衣橱 与规格“我”不符失败；实现 canonical UUIDv7/default workspace 和成员命令后通过。
- GUI create/edit/switch/reload：缺少新成员名称入口失败；加入 MemberSwitcher 后通过。
- 旧库 fixture 自定义成员名称：迁移错误变成“我”失败；保留自定义名字后通过。
- unsupported_profile_fields：role 被静默丢弃失败；按成员操作 allowlist 拒绝后通过。
- GUI pending member.select：保存单品仍 enabled 失败；组件 onBusy 禁止父表单保存后通过。

验证范围：成员创建、重命名、选择在 Application 重启后保留；UUIDv7 空间与成员；default 别名输出 canonical UUID；两成员衣物 list/get 隔离；错误 member_id 拒绝；CLI 创建成员和 MCP 跨成员 get 拒绝；GUI 创建/改名/切换/录入/刷新完整流程。迁移通过 Application 查询验证衣物 ID、时间戳、归属与旧自定义名字保留。旧 audit/outbox 行不删除，仅 canonicalize embedded member_id。

GUI 320/375/414/768/1024/1280/1440 实测成员控件可见且无横向溢出；键盘 Enter 打开管理 details；沿用 design.md Atelier tokens，原生 select/form、48px 目标、error/status 文本、无新增动画/图片/hover-only 操作。Hallmark component-scope 检查适用门槛通过。

命令：cargo test --workspace；cargo build -p wardrobe-cli；npm --prefix apps/desktop run build；WARDROBE_TEST_PORT=5184 npm --prefix apps/desktop run test:e2e。独立端口为并行 agent 运行避免冲突。原有分页 fixture 和断言保留。

历史查询由 #4 clothing.history 公共边界接入成员解析器；此票不重写 parallel clothing.rs。跨平台 native GUI 安装与完整 Beta 发行验证不属于本次已执行证据。
