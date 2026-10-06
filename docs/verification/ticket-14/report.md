# #14 F07 商品展示图实施证据

2026-10-07，基于9070fa7，已合入当前集成73a7f08（含f6d8749 DB9真实穿着）。实施分支 codex/ticket-14-product-display，工作树 /tmp/wardrobe-ticket-14。复用已完成 #7 debug 编译缓存，未改 release 产物；原独立未完成target596MiB已清理，终态后debug incremental482MiB已清理，验证期间target软链接供真实开发CLI桥，终态移除该链接，源码工作树干净；共享缓存保留。

## 已验证

- 三个逐层 red→green：专用adapter配置被旧API拒绝→保存/重读且不允许text假验证；product.generate旧APIunsupported→真实SQLite幂等任务与旧图不变；product.process旧APIunsupported→真正HTTP模型发现、单图调用、解码、预览、原子采用及重启。
- Application 六项商品图行为测试通过：配置分离、上传许可/幂等、回环真实请求响应/预览采用/重启、请求失联等待人工核对禁止重发、运行中取消后迟到成功结果不能入库、明确client scope/member约束。fixture无API key，无实际云供应商调用。
- CLI/MCP 商品图工作流测试通过：真正独立CLI创建队列、重复同意图仅同job ID、MCP授权取消、CLI重启读同持久状态，未改变旧图。其余共用入口仍薄适配器。
- `cargo test --workspace`：92 passed、0 failed；2 ignored（OS凭据实机、真实模型网络smoke由对应票据另有证据，未在本次重复）。
- `cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all`、TypeScript/Vite构建通过。
- 与 #9 联合完整GUI回归29 passed、2 skipped（真实模型下载和本地模型服务器需显式环境）；商品图浏览器→真实CLI→Application→真实回环HTTP→预览采用→重载通过，七宽度320/375/414/768/1024/1280/1440无横向溢出、关键按钮可用。专用模型设置通过GUI保存/重读 adapter、图片能力和安全凭据入口，不显示文本验证按钮。
- `product-320.png` / `product-1280.png` 来自上述实际GUI，不是设计稿；图片是原创协议fixture色块，不能据此声称衣物色彩/版型质量已验收。已查看截图并复用原check-label修正勾选控件；使用Atelier已有tokens，不增加颜色/字体。

## 必须保留的未完成验收

#14仍OPEN，真实供应商付费图像调用与实际衣物结果质量未验证。需用户在安全设置输入自己的key、确认Flash模型开通，在明确照片授权后调用并查看结果；核对颜色、版型、印花、纹理后采用及重启。当前证据不等于该验收，也不证明试穿/搭配生成等能力。

未运行最新原生GUI应用包或三平台此代码CI，交给集成流程后验证。完整Beta/安装升级门槛继续由父票据追踪。没有新增数据库表/版本。额外联合验证：先记录 WearEvent 后生成并采用，旧wear snapshot/asset仍相同；并发更换主图会拒绝过时采用，重新明确选回原主图后才允许采用。

## 实际行为边界

只有所选一张本机原图规范化上传，源文件SHA与供应商输入限制校验；上传不带EXIF/路径。默认专用Seedream5.0Flash，省略不支持的组图参数，只请求单张base64。GET models返回记录不足以证明账户有额度，真正生成成功才确认本次能力。若模型发现接口不可用，明确失败且没有生成，而不是偷偷猜端点或账单。

请求提交前persist submitted_at；同步接口无图片query API时未知结果waiting_input，同job不能重发。取消只保证本机旧图/事实不变，供应商调用可能已经计费。重新生成按钮为新的明确调用意图。采用有base_display_asset_id并发检查，同一事务采用asset/更新衣物/job/audit/outbox；预览不进入同步manifest。原始资产与历史保留，不覆盖事实。
