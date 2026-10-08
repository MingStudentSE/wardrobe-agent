# #12 真实本地模型连接验证

2026-10-07，macOS 26.6.2 / arm64；基于 integration 1861c33。规格05允许本地模型，本次验证真实权重推理，覆盖 #12 文本/视觉连接，不缩减完整 Beta 及后续图像生成/试穿范围。没有使用模拟响应、拦截请求、云供应商账户、用户密钥或用户衣物照片。

## 可核对的模型与运行时

- [ggml-org 官方转换仓库固定 revision](https://huggingface.co/ggml-org/SmolVLM-256M-Instruct-GGUF/tree/b9e4379657e1450d04d02eec8e345667265b0a00)，原模型 HuggingFaceTB/SmolVLM-256M-Instruct。模型卡及官方 metadata 均标为 Apache-2.0。只下载以下两个 Q8_0 文件，未下载 f16/其他模型或 clone 整库。
- LM：175054528 bytes，SHA256 `2a31195d3769c0b0fd0a4906201666108834848db768af11de1d2cef7cd35e65`。
- projector：103769856 bytes，SHA256 `7e943f7c53f0382a6fc41b6ee0c2def63ba4fded9ab8ed039cc9e2ab905e0edd`。
- 实际下载 SHA256 全部与官方 metadata 一致。合计278824384 bytes = 265.91 MiB，低于600 MiB权重预算。模型卡和 Apache 许可副本保留在隔离运行目录。
- [llama.cpp 固定 commit](https://github.com/ggml-org/llama.cpp/tree/abeada335e2e78bd3fe63febafab7e900ce75810)，MIT许可。其[多模态官方文档](https://github.com/ggml-org/llama.cpp/blob/abeada335e2e78bd3fe63febafab7e900ce75810/docs/multimodal.md)明确支持SmolVLM-256M，实际源码 server 文档支持 base64 image_url。固定源码 archive 校验及配置见 provenance.json；archive 构建显示0.6.0-dev/build0/commit unknown，源码身份由固定URL和 archive hash记录，未伪造版本输出。
- 隔离 `/tmp/wardrobe-local-provider` 内 CMake Release，目标仅 llama-server，编译-j2，Metal/BLAS关闭，测试和示例关闭。首次预构建UI gzip资源失败；关闭官方 `LLAMA_BUILD_UI` / `LLAMA_USE_PREBUILT_UI` 并移走本次构建的stale UI资产后成功，未修改上游源码、未全局安装。
- 服务以最小环境启动，无继承供应商认证变量。绑定127.0.0.1:18081，alias wardrobe-smolvlm，2048 context、单slot、CPU generation/prefill threads=2、GPU层0、projector CPU。`/health`实际ok，`/v1/models`实际声明text/image输入、text输出和Q8_0模型。

## 实测结果

使用仓库可复跑的 `scripts/verify-local-provider.py`，先向真实服务预热两种请求，内容与应用验证器完全相同；视觉输入仅为应用原创64×64内置PNG蓝色方块。随后通过真实CLI加载Application、保存authentication=none模型至临时SQLite，登记仅provider.validate的限定客户端。token只存在内存和stdin，不出现在argv、报告或日志；无credential写入。

| 路径 | 文本 | 视觉 |
|---|---|---|
| 直接预热真实推理 | 0.009s，实际输出` I`；usage 12+1=13 | 1.938s，实际输出` Blue.`；usage 154+3=157 |
| 限定客户端 CLI / Application | 0.022s，verified=[text] | 0.023s，verified=[vision] |
| 限定客户端 MCP / Application | 0.015s，verified=[text] | 0.022s，verified=[vision] |
| GUI→真实开发CLI bridge→Application | 文本连接成功 | 视觉连接成功 |

Application四次验证均在20秒窗口内成功，返回usage与所选能力，未宣称声明能力即验证。预热后服务允许prompt cache，后续耗时不能当作未缓存视觉吞吐指标。文本验证max_tokens=1，实际单token` I`只证明兼容非空响应，不表示模型遵循“Reply OK.”或具有衣橱任务质量。视觉真实回复` Blue.`与内置图颜色一致，也不构成识别准确率、JSON提取或生成试穿验收。

实际GUI Chrome E2E 1 passed；仅显式点击发起文本、视觉验证。七宽度320/375/414/768/1024/1280/1440均无横向溢出，截图320已人工检查，保持现有Atelier设计。测试仅在显式设置 `WARDROBE_REAL_LOCAL_PROVIDER` 时运行，未设变量的skip不作为通过证据。

- [实际模型/usage/API返回](real-inference.json)
- [固定版本/权重校验/配置/清理](provenance.json)
- [真实服务加载与推理日志](server.log)
- [320px](real-local-320.png)、[1280px](real-local-1280.png)、[模型卡](model-card.md)

## 复跑与清理

```sh
# 已校验的隔离运行时；CPU限制2线程，纯loopback，无API key。
env -i PATH=/usr/bin:/bin:/usr/sbin:/sbin LANG=C TMPDIR=/tmp \
 /tmp/wardrobe-local-provider/build/bin/llama-server \
 -m /tmp/wardrobe-local-provider/weights/SmolVLM-256M-Instruct-Q8_0.gguf \
 --mmproj /tmp/wardrobe-local-provider/weights/mmproj-SmolVLM-256M-Instruct-Q8_0.gguf \
 -c 2048 --parallel 1 --host 127.0.0.1 --port 18081 --alias wardrobe-smolvlm \
 -t 2 -tb 2 --n-gpu-layers 0 --no-mmproj-offload
# 另一个终端，在工作树中指定其实际CLI路径；本机媒体dylib环境按工程说明配置。
python3 scripts/verify-local-provider.py --cli /absolute/path/to/wardrobe-cli \
 --output /tmp/real-local-inference.json
# GUI opt-in：构建当前CLI后，在desktop运行
WARDROBE_TEST_PORT=5189 WARDROBE_REAL_LOCAL_PROVIDER=http://127.0.0.1:18081/v1 \
 npm --prefix apps/desktop run test:e2e -- local-provider.spec.ts
```

实际本轮服务已SIGINT正常停止并确认18081不再监听；runner临时SQLite（含disposable token hash）已由TemporaryDirectory删除；GUI临时配置数据库也已按本轮唯一模型名称与端点定位后删除。未创建凭据，未写用户数据库。权重/源码/build保留在隔离目录供后续复测；全部实际分配约556 MiB（精确KiB见provenance），其中权重265.91 MiB，不能将磁盘大小当作RAM需要量。

本次补齐真实本地文本/视觉连接证据；不替代云供应商账号、安全凭据平台矩阵或后续图片生成/试穿适配器实际调用。没有产品代码修改，无需为证据引入实现镜像测试；所有成功结论来自上述真实运行。
