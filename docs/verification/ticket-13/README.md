# #13 AI识别与人工纠错验证

范围F05/F30：共享 Application、持久SQLite DB12、GUI、CLI/MCP贯通；模型描述/词汇规范化候选仅为建议，经逐项编辑确认才写事实。新任务从 integration f6d8749 建立独立 codex/ticket-13。

## 已运行证据

- 红绿日志保留第一unsupported_operation、HTTP worker、batch草稿、PNG传输、taxonomy候选、审核幂等以及取消中429响应复活队列的失败，再记录修复后的真实终态。测试边界是公共Application+临时SQLite，模型服务仅在外部HTTP边界设fixture，不mock事实读写。
- core recognition7项：明确provider/图片/版本/上传确认与精确幂等；真实HTTP返回proposal、taxonomy审核、审核幂等及重分析不覆人工；批量失败手工提交；提交后重启不发第二次POST且成员隔离；截断HTTP响应持久未知+取消后禁止retry；HTTP200和429返回前取消都不能发布或恢复队列；模型自报0.7不当作准确率、歧义候选弃权，以及版本12实际缺表重启修复。
- CLI/MCP新增测试：真实进程、授权token、HTTPfixture、共享job/proposal IDs、MCP处理/审核、CLI重启读取一致；最终联合CLI入口18项全绿。
- Chrome通过开发bridge调用真实CLI/Application；GUI fixture验证确认发送、编辑审核写事实、重新分析保留人工值、320/375/414/768/1024/1280/1440无横向溢出；批量单项离线失败后仍入库且另一草稿保留；opt-in真实SmolVLM通过GUI返回公开鞋图建议。截图 review-320/1280 与 actual-gui-shoe。
- 原生Tauri GUI本轮没有直接操作；Chrome真实bridge不是原生窗口实测。HTTPfixture不是实际模型推理，分开报告。

## 真实模型质量与失败

本地 llama.cpp CPU2/context4096，SmolVLM256M Q8及匹配projector，model=wardrobe-smolvlm，端点loopback18083，权重复用既有许可样本环境，无用户key/照片。所有实际CLI输入为公开许可fixtures：OKM Shirt Front(CC BY-SA4.0)、Zapato/Jeans(publicdomain)，邻接LICENSE注明来源。

首次自然语言直接实验输出“White shirt with a baseball on it.”：类别/白色合理，印花错误。严格JSON实验虽然语法合法，输出category=shoes/name=ILIA，判为质量失败。`ticket13-actual-first.json`/`ticket13-actual-structured.json`保留原文。初次真实Application发送WebP被服务400拒绝（`actual-webp-failure.json`），随后通用规范化最长边512px PNG解决格式兼容；来源哈希与传输PNG哈希/尺寸独立保留。

`actual-cli.json`为真实CLI→Application→HTTP→模型，不是预置响应。鞋描述“black and white sneaker with white laces”、裤子“blue jeans pant”有实用类别/颜色信息；白衣描述为shirt但猜测IMUA collection，仍是低质量/需人工核实项。不得把成功HTTP、合法JSON或两样本可用结果宣传为准确率。描述中的品牌不会进入规范化候选；模型原始描述及不确定性提醒可见，人工需要改写错误细节后再确认。

## 复现

构建依赖使用PKG_CONFIG_PATH=/tmp/wardrobe-media-native/install/lib/pkgconfig，DYLD_LIBRARY_PATH同路径的lib；CARGO_INCREMENTAL=0/CARGO_BUILD_JOBS=2。执行cargo test -p wardrobe-core、cargo test -p wardrobe-cli、cargo clippy --workspace --all-targets -- -D warnings、cargo fmt --all -- --check；desktop npm run build/test:e2e。真实GUI额外设置WARDROBE_RECOGNITION_REAL_ENDPOINT=http://127.0.0.1:18083/v1，模型需预先启动。默认测试跳过真实模型，不声称skip为成功推理。

最终合并 root 4fa138f（包含 #14、#20、#26），DB 最大版本12，同时实际初始化 recognition 与 encryption。终态 `cargo test --workspace --locked`：110 passed / 8 ignored；`cargo clippy --workspace --all-targets --locked -- -D warnings`、fmt check、desktop build 均 exit 0。联合 GUI 37项：34 passed / 3 skipped，其中本地真实 SmolVLM 鞋图测试通过。8项 Rust ignored 与3项 GUI skipped 为既有需显式真实模型或OS凭据测试，本轮不据此声称原生安全存储通过；#26 独立 merger 的9项真实 Keychain证据见 ../ticket-26/merger-keychain.log。

最终终态日志在 [logs](logs/)，含 encryption-joint-workspace.log、encryption-joint-gui.log、clippy-final.log、fmt-final.log、ui-build-final.log，所有命令实际退出0。红绿失败日志完整保留。320px最终截图已实看：选择框20px/触控标签48px，颜色两列含米色/多色均完整可读。Chrome实际Application bridge、CLI/MCP及公开样本实际推理已验证；原生Tauri窗口本轮未操作，跨平台安装包识别与更强模型质量仍需对应验收。未 push 或关闭Issue。

模型服务在本轮证据完成后停止；权重、编译运行时与复现脚本保留在既有 /tmp/wardrobe-local-provider 与本目录。实际样本可用建议与错误结果并列保留，不以小样本推断准确率。

独立预审发现：POST 前 submitted 状态 CAS 检查 running/token 但尚缺 lease_expires 到期约束；独立 merger 将补到期守卫与过期 lease 不发 POST 回归。此实现 tip 等待该修复及独立审查，不声明 Issue 已完成验收。
