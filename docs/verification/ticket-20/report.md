# Ticket 20 / F15 验证记录

实现真实 Ark Seedream 多参考图请求、模型预检、上传授权、持久任务及未知结果防重复提交；保存版本全量来源追溯、解码和哈希校验、本机 WebP 预览及明确采用。保留离线拼贴，失败不改变已保存搭配或穿着。

当前证据：5 个 core 行为测试；新增真实 CLI→MCP→core→HTTP 多图测试；GUI 通过实际 CLI bridge、实际 HTTP fixture 验证双图上传、预览、采用及重启读取；320/375/414/768/1024/1280/1440 宽度无横向溢出。截图 flatlay-320.png、flatlay-1280.png 已人工查看，彩色结果是协议 fixture，不能作为云图质量证据。原商品展示图 6 个行为测试继续通过。

官方协议与范围见 ADR0015。无用户供应商密钥，未调用真实付费云模型；真实多图生成与人工质量检查仍是未完成验收门槛，ticket 不应关闭。GUI 当前验证浏览器开发链路，不冒充打包桌面三平台验证。

最终全量验证：cargo test --workspace --locked 为 98 passed / 2 ignored（OS 凭据及真实模型 gate）；cargo clippy --workspace --all-targets --locked -- -D warnings 与 cargo fmt --check 通过；npm build 通过；完整 Playwright 为 30 passed / 2 skipped（显式真实模型 gate）。新 GUI 测试独立成员，避免污染分页数据。
