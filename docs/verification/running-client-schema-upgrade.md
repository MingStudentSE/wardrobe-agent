# 运行中的旧客户端与数据库升级

2026-10-07，集成提交4ffb63b。

真实 SQLite 外部行为回归：先打开 Application，再以独立 SQLite 连接提高 user_version，旧 Application 的成员创建、衣物创建和成员读取必须返回 unsupported_schema，数据库成员数量不变且衣物数量为0。

新增测试首次运行失败：member.create 实际返回成功并创建成员，证实旧连接仅在启动时检查版本。修复后 workspace 16 个测试通过，clippy --all-targets -D warnings通过。

Application.execute 复查当前版本；共享 begin_write 在取得 IMMEDIATE 写事务锁后再检查版本，避免检查和写入之间发生另一个进程的 schema migration。成员与衣物写入已使用该入口；合入资产和模型模块时必须沿用。

测试位置：crates/wardrobe-core/tests/application.rs 的 open_client_refuses_operations_after_newer_client_upgrades_database。
