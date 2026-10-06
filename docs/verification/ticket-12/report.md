# #12 模型配置与安全凭据验收记录

2026-10-07，macOS Apple Silicon。实现分支 codex/ticket-12，基于 integration 36d902c，合入 3f1c4c1（#4、live-client schema guard与#5媒体/原生依赖）。

已运行的证据：

- TDD runtime red：provider.save / provider.credential.set / provider.validate 原先 unsupported_operation；分别实现后 green。CLI stdin 输入原先 invalid_json/null；实现 --input-stdin 后跨 CLI/MCP配置往返成功。凭据 argv 原先 not_found，限制后 secret_requires_stdin。端点更换原先仍 configured=true，绑定+清理后 false。
- SQLite Application tests：配置重启往返、无 key手工录入、拒绝非本机 HTTP/URL嵌入凭据/查询参数/未知生成能力/普通配置中的 api_key。
- 真实 loopback HTTP server验证实际 POST /v1/chat/completions 和配置模型，返回已验证 text；声明 vision不冒充已验证。这个 fixture 不是供应商账号调用。
- `cargo test --workspace`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo fmt --all -- --check`。
- 明确运行 ignored OS smoke：`cargo test -p wardrobe-core --test providers credentials_can -- --ignored`，真实 macOS Keychain 写入、查询、端点更换清理、删除。使用临时生成的测试值，输出不含测试值；SQLite/WAL文件字节扫描无测试值；RAII在断言失败时清理。历史 red产生的一条测试凭据已按确切 account 删除。没有读取用户密钥。
- 真实 Chrome GUI E2E 11项通过（原衣橱/成员/生命周期测试保持通过），新增模型保存、重载选择、无认证本机设置，不 mock 后端。模型设置在 320/375/414/768/1024/1280/1440宽无水平溢出；截图 model-settings-320.png、model-settings-1280.png。`npm run build` 通过。
- CLI新输入方式避免密钥在命令历史和进程参数中；开发桥由旧 argv改为stdin。Application输出、供应商错误和配置列表均不回传key。

尚未判通过的门槛：

- Windows Credential Store 和 Ubuntu已解锁 Secret Service实测，三平台发行包/运行依赖。#5的Tauri async + spawn_blocking已合入源码，原生运行验证仍未代替。
- 用户真实供应商文本/视觉调用，专用图像/试穿适配器及真实账号成功；文本fixture与配置界面不替代这些门槛。
- #27 客户端scope/完整授权。当前原有CLI/MCP直连API并没有虚构授权层。

因此建议合入基础实现但保留 #12 的跨平台、供应商、授权验收项。scope基础由独立slice继续实现，不为等待#27造成循环依赖而取消该验收。完整 Beta 继续依赖后续集成验证。

最终媒体合并检查使用 PKG_CONFIG=/opt/homebrew/opt/pkgconf/bin/pkg-config、PKG_CONFIG_PATH=/tmp/wardrobe-media-native/install/lib/pkgconfig、DYLD_LIBRARY_PATH=/tmp/wardrobe-media-native/install/lib；sha2与媒体模块统一0.11，其digest字节转换保持原来的SHA256账户fingerprint。
