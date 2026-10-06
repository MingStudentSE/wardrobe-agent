# #6 / F02 批量录入验收

2026-10-07，基于集成 d0b22ef，随后合并最新集成分支再复测。本次为真实Rust Application、SQLite、文件解码、CLI进程与浏览器GUI测试；系统文件选择器是唯一替换的外部边界。没有模型key、云请求、模拟业务后端或个人照片。

实现与实际证据：

- 创建前保留原始字节；删除原始输入文件后关闭并重新打开Application，process仍生成真实压缩展示图。同operation_key重复创建返回相同批次，source不可读项可以恢复源文件后明确retry。
- 默认每文件草稿，100文件成功、101拒绝；21项归并拒绝。混合良图/损坏图正常处理其余项，逐项取消/重试不影响别项。草稿名称/类别/材质修改跨重启保留，后续patch保留先前事实。
- 两独立SQLite连接同时claim只一成功；取消使旧lease完成失败；关闭连接、实际等待1.05秒、重新连接后过期lease可接管。并发入库返回相同clothing ID、仅一条单品历史。上述是实测连接重启和竞争，没有声称已经在原生GUI上强制杀进程验收。
- 同草稿入库经关闭重开及CLI→MCP→CLI重试返回相同衣物、资产采用进入现有展示图同步白名单，原图/任务输入保持本地。
- 新operation descriptors包含输入/输出schema，限定scope授权读可读同成员队列，处理权限缺失拒绝，跨成员读拒绝。无授权CLI/MCP既有回归保持通过。
- 通用worker seam的429可重试分类实测3次上限与真实到期延时，401永久分类保持failed。本地batch无网络任务，不把状态策略测试称为真实供应商账户请求。
- GUI真实CLI保存任务，reload恢复草稿，归并两图、保存名称、reload保留、入库与再次reload不重复；超60MP图片独立报错。另一测试取消一项、reload保留取消、处理另项、明确retry后完成该项。
- GUI七视口320/375/414/768/1024/1280/1440均实测无横溢出，复用Atelier tokens、48px操作、键盘/触摸按钮。实际截图 `batch-320.png` / `batch-1280.png`，合成颜色fixture仅验证管线，不冒充衣物质量样本。

运行命令：配置本机独立libheif1.23.1前缀后 `cargo test --workspace`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`；desktop `npm run build`、`WARDROBE_TEST_PORT=5186 npx playwright test`。GUI完整20项通过；Rust计数与合并后的最终结果在下方更新。

原生安装包、系统文件选择器和三平台完整Beta验收继续归 #35，不把浏览器bridge验证当作已安装原生GUI证据。当前Mac屏幕锁定不影响上述独立代码/SQLite/CLI/浏览器测试。

最终自动化结果：Rust workspace **62 passed / 0 failed / 1 ignored**（ignored为既有需真实OS安全存储的smoke，本次未重新声称执行）；fmt通过、clippy -D warnings通过；前端tsc/Vite构建通过，GUI完整 **20 passed / 0 failed**。七宽度截图已经查看320px实际图，修复归并checkbox继承全宽的问题；交互文字单行、按钮可见。真实CLI/MCP批次、查询、逐项取消/重试、原文件删除后处理、同草稿跨入口重复入库均包含在62项实际运行测试中。
