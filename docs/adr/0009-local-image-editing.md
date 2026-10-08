# ADR 0009：不可变图片衍生与本地模型抠图

状态：实施中；跨平台安装包验收待完成。

每次旋转、恢复与抠图创建新 Asset ID，沿用本机原图引用，保留旧 Asset 与展示 blob。处理成功仅生成 preview；用户明确替换或添加后才改变单品关联。历史 Outfit snapshot 按旧 Asset ID 继续读取原展示图。抠图失败不修改原图、旧展示图或衣物事实，恢复时本机缺失原图返回 `original_unavailable`。

采用固定 U²-NetP ONNX（rembg v0.0.0 release，4,574,861 bytes，SHA256 `309c8469258dda742793dce0ebea8e6dd393174f89934733ecc8b14c76f4ddd8`）与纯 Rust tract-onnx 0.23.8 CPU runtime。无 Python、云 API 或额外供应商密钥。320×320 RGB/最大值归一化、ImageNet mean/std、主输出 min/max alpha、Lanczos 返回展示尺寸；透明图保持 alpha。它是显著物体分割，不能保证每幅图都去除背景或达到发丝级边缘；生成后须预览，不自动采用。

下载和推理使用 #6 持久任务与租约；网络/推理不持有 SQLite 写事务。成功衍生入库和 job 完成同一事务，失效租约不得入库；展示 sync manifest 继续仅白名单元数据，不含本地路径、原图、模型缓存或处理记录。

## 模型授权与出处

作者确认 released codes **and models** 使用相同 Apache-2.0，而不是从 rembg 的 MIT 推断权重授权：[作者回复](https://github.com/xuebinqin/U-2-Net/issues/208#issuecomment-847883906)。模型获取源：[rembg release](https://github.com/danielgatis/rembg/releases/download/v0.0.0/u2netp.onnx)，来源项目：[U-2-Net](https://github.com/xuebinqin/U-2-Net)，论文：[Pattern Recognition 2020](https://arxiv.org/abs/2005.09007)。下载 URL、字节数、SHA256、版本和授权在 API/UI 可见，完整 Apache 文本保留 `third-party/licenses/U-2-Net-Apache-2.0.txt`。

作者同时提醒检查训练数据集条款，并[确认 U2Net/U2NetP 用 DUTS 训练](https://github.com/xuebinqin/U-2-Net/issues/208#issuecomment-848093240)。[DUTS 官网](https://saliencydetection.net/duts/)说明训练图来自 ImageNet DET，标注版权 all rights reserved；[ImageNet 正式访问条款](https://image-net.org/download.php)要求访问者只为非商业研究与教育使用 Database。本项目不获取或分发这些训练数据库；这些 Database 条款是否扩展到作者另以 Apache 明确授权的已有模型权重，来源没有作出确定说明。不能据此声称商业使用已完全清除所有训练数据风险。当前免费 Beta 按作者明确模型许可提供下载与引用，保留上述边界供发布审查。
