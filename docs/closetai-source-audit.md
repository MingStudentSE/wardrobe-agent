# ClosetAI 源码审计

审计日期：2026-10-06（Asia/Shanghai）。仓库 https://github.com/Ligoml/ClosetAI ，commit `2295a319e7fc3646e8e8856bbbf5db57a3b0e033`。静态源码审计，未编译、未调用模型，不代表真机或 API 验收。

## 产品与实现

SwiftUI + MVVM + CoreData 的 iOS 应用；`project.yml:1-18` 将最低系统设为 iOS 26。不是跨平台桌面应用，也不是可直接复用的 Rust 核心。

| 能力 | 源码证据 | 实际含义 |
|---|---|---|
| 拍照/图片录入、标签识别 | `ClosetAI/ViewModels/WardrobeViewModel.swift:133-169`; `ClosetAI/Services/AliyunService.swift:73-141` | 原图经 Qwen-VL 云 API 自动打标，失败仍能保存“其他”分类 |
| 本地抠图、单品白底图 | `ClosetAI/Services/ImageProcessingService.swift:33-119` | Vision 前景掩膜 + 本地白底缩放居中；不是生成式服装展开、透视纠正或遮挡补全 |
| 原图与衍生图分别保留 | `ClosetAI/ViewModels/WardrobeViewModel.swift:144-159` | 原图和 flat-lay 独立路径，值得继承 |
| 手动搭配 | `ClosetAI/Views/Outfit/ManualOutfitView.swift:15,182-195` | 最多 4 件；界面要求 API key 后才完成选取，离线手动流程因此受限 |
| AI 搭配图 | `ClosetAI/ViewModels/OutfitViewModel.swift:38-74`; `ClosetAI/Services/AliyunService.swift:215-237` | 用户选定衣物后上传图片，Wan 生成；失败回退本地拼贴 |
| 虚拟试穿、结果保存 | `ClosetAI/ViewModels/OutfitViewModel.swift:79-100,133-169`; `ClosetAI/Services/AliyunService.swift:256-264` | 真人图与衣物图上传至云 API；是生成效果预览，不能保证真实尺寸/合身程度 |
| 分类、颜色、图案、季节、场合 | `ClosetAI/Models/ClothingModels.swift:6-104` | 外套、连衣裙、包独立类别，标签可编辑；容错解析字符串/数组 |
| 安全凭据 | `ClosetAI/Services/AliyunService.swift:12-47` | Security framework Keychain；写入返回码未检查 |
| 删除恢复 | `ClosetAI/ViewModels/WardrobeViewModel.swift:176-194` | 单品软删除/恢复；穿搭删除为硬删除，不能原样作为同步语义 |
| 实际穿着记录 | `ClosetAI/ViewModels/WardrobeViewModel.swift:210-216` | 单品计数、日期、WearLog 都存在 |
| 久未穿统计 | `ClosetAI/ViewModels/WardrobeViewModel.swift:79-118` | 实际按是否加入已保存搭配及搭配 createdAt 判断，未使用 WearLog，口径不正确 |

## 必须纠正的 README 宣称

1. README 写“AI 根据场合、季节、数量智能组搭”。当前入口 `OutfitView.swift:47-58` 进入手动选衣；AliyunService 当前仅有识别、图片生成、试穿，无文本推荐方法。当前版本没有找到该自动组搭的完整实现，不应作为已经具备的推荐引擎。
2. “all on-device”不能解释为 AI 推理本地：`AliyunService.swift:65,101,161,175` 明确请求 DashScope 的 qwen-vl-plus 与 wan2.6-image。
3. 单品 flat-lay 是本地画布合成，AI outfit flat-lay 才是生成模型；两者必须区分。
4. README 有 MIT 徽章及 LICENSE 链接，但 `git ls-files` 没有 LICENSE。审计时只能确认作者宣称 MIT，正式复制代码前需补全版权与许可文本。

## 具体风险与设计启发

- `AliyunService.swift:156` 使用 `images.prefix(4)`；试穿方法把真人放在第一个输入后拼衣物。如果选 4 件衣物，5 张输入会截去最后一件。这是代码输入截断风险，不是已复现实测结论。我们应声明 provider 最大输入数并在调度前校验，禁止静默截断。
- 图片清理、CoreData 写入是多步操作，失败可产生孤儿文件；使用 staged assets 与任务状态，提交后挂接资产，定期回收孤儿。
- 模型/端点硬编码；需要 capability-based provider，识别、文字推荐、商品图、搭配图、试穿分别声明能力。
- 图片提示词强调保持原色、版型、图案及每件只出现一次，有参考价值，但不构成生成结果的保证。我们需输出资产来源、输入单品 ID、模型、任务记录及用户确认状态。

## 吸收范围

继承：原图/衍生图分离、本地抠图、白底展示、分类细分、手动选衣、AI 多图合成、本地拼贴兜底、试穿前后对比、Keychain 思路、单品回收站。

重做：推荐引擎、真实 WearEvent 统计、完整离线手动流程、多端同步、跨平台存储、provider 抽象、任务恢复、输入数量校验。暂不复制 Swift 源码，许可证缺失问题需单独解决。
