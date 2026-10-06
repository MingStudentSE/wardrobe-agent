# Inventory Application API performance baseline

2026-10-07，macOS26.6.2，Mac16,10/aarch64，10 physical cores，16GiB RAM。Rustrelease，独立新SQLite，10000单品，30次真实组合search/color/season/tag排序首屏及实体update。

实测search p95=117.200ms，save p95=0.197ms。命令：cargo run -p wardrobe-core --release --example benchmark_inventory -- <new-benchmark-database>。代码验证每次首屏24项，使用共享Application API，未替换数据库。

这是当前机器的库存基线；不代表规定4核/8GiB基线、三平台、100000穿着事件或GUI冷启动已验收。完整门槛仍待#35按每平台环境执行。基准不含图片或网络。
