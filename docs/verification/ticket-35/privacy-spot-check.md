# 隐私抽查（issue #35）：发行包无凭据 / 无 EXIF/GPS / 无原图 / 无模型权重内置

日期：2026-10-08。对象：
- DMG 内 `电子衣橱.app`（挂载只读后全量扫描，14 个文件）
- 独立 CLI portable 目录（11 个文件，与 sidecar 同源二进制 SHA256 `f2d7258c…`）

方法与脚本：`results/continue-20261007/ticket35/privacy-scan.py`（随证据归档），报告 `privacy-scan.json`。

## 结论：overall_clean = true

| 检查项 | 结果 |
| --- | --- |
| 凭据模式（sk- style key / AKIA / 私钥块 / Bearer 字面量 / password= 字面量 / 64hex 常量） | 8 处原始命中，**全部甄别为公开构建元数据，0 处未决**：① 64hex = SQLite amalgamation `SQLITE_SOURCE_ID` 版本哈希（位于 sqlite 字符串常量区，上下文核验）② U2NetP 模型公开 SHA256（随包 NOTICE 明文文档本身）。分析后逐条分类写入 `privacy-scan.json` 的 `credential_hits_analyzed` |
| EXIF / GPS | 包内唯一图片为 `Contents/Resources/icon.icns`（57KB，应用图标，无 EXIF/GPS）；无 JPEG/PNG/HEIC 照片文件 |
| 原图 / 大图 | 无 >1.5MB 图片；无任何用户照片或测试样图（fixtures 不入包） |
| U2NetP 模型权重 | 包内无 `.onnx`（运行时按固定 URL + SHA256 下载校验） |
| WebP/其他资产 | dist 前端静态资源为构建产物，无图片资产夹带 |

## 说明

1. 扫描为字节级模式扫描 + 上下文人工甄别；`credential_hits_analyzed` 保留原始命中与分类，不隐藏原始结果。
2. 运行时用户数据边界（原图/真人图/试穿图仅本地、同步白名单等）属于业务验收（#27/#28 等，已关闭并留证据）；本抽查只针对**发行包内容物**。
