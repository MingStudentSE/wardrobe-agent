# #26 独立合并验收

2026-10-07，macOS arm64，独立工作树 `/tmp/wardrobe-merge-26-export`、`codex/merge-26-export`。固定基线 root52b4ae3，合入实施者最终1ab189172e450c7c9a442ee02ab08bd224f8544e，保留rootea7a277与care13/insights14。独立执行日志在independent-logs；实施者README与日志另存，不作为本轮通过依据。

## 规格：Issue26四项条件

1. 建立空间、两阶段第二安装配对、恢复密钥/密码恢复与安全保存：六项实际临时OS Keychain行为独立运行，覆盖重启、单次邀请消费、并发确认、改密码恢复、失败轮换原子性、历史epoch与DB/WAL无明文私钥。OS只保存随机32字节解锁钥，SQLite钱包AEAD密文；没有读取用户钥或照片。
2. 真实字节往返与拒绝：默认公开Application envelope行为独立通过，验证错误密码、kind/space关联数据、截断/篡改、nonce长度、协议版本及随机nonce；实际第二安装验证独立外部文件包，签名损坏、对象损坏、25MiB文件、目录symlink拒绝。损坏展示图导出失败且不回退原图、不发布新目录；已知撤销/epoch更新拒绝旧包。未知撤销的离线peer仍可读已知旧包，未宣称远程擦除。
3. 算法与远端边界：ADR0012记录XChaCha20-Poly1305、HKDF-SHA256、Argon2id及固定参数，ADR0019记录应用白名单协议。独立实际CLI创建两个临时安装，采用展示图与完整搭配/穿着快照、care三修订及配置、insights89天偏好通过独立外部目录返回；扫描3个外部文件没有事实名称/备注、原图字节、源文件名或本机路径。peer facts仍为空、prefs仍90：读取是inspection，不是同步或导入。实际full客户端两操作scope_denied，授权MCP tools/list无encryption工具，可信CLI输入走stdin。
4. 外部行为与领域证据：本轮真实Application/SQLite、实际OS、CLI/stdin MCP及Chrome开发bridge检查，见下文。GLOSSARY与ADR0019已包含密文文件包边界。没有发现需要修复的独立规格缺陷；实施者历史红绿保留，但本轮不把它们当自己的失败。

## 标准审查

保留Atelier组件、Hallmark六轴critique、既有48px入口与键盘/触控表单，不依赖hover或drag。GUI目录输入可触控，原生picker为补充；pending ref保护picker/操作互斥。数据路径只经共享Application；IMMEDIATE事务给白名单事实与资产清单同一读取状态，字段逐项选择，任务/供应商/客户端凭据/钱包/恢复材料/原图/本机路径不导出。历史非主图从immutable outfit_versions与wear/care引用授权，不把任意preview放开。数据库无新表/版本，保留DB14最高值及各实际表初始化。

有界文件读取最多24MiB+1，Unix额外stat/open dev+ino检查；没有声称实测对抗TOCTOU竞态。对象长度/hash/AEAD/package-object身份与raw签名清单校验后读取；随机文件名不会暴露图片ID/hash。最多10000对象、16MiB明文对象、256MiB密文总量，真实1700件×10000备注超限没有发布或截断，不能用单包限制缩减完整Beta10k衣物/100k事件规模。#27需实际分页/分片、transport/outbox/inbox/merge。

## 独立终态

- workspace：140 passed、0 failed、11 ignored；CLI21通过。ignored不计成功。
- 额外真实OS：9 passed（6 encryption + 3 package）；临时钥测试后清理。
- clippy workspace/all-targets -Dwarnings、fmt、git diff检查、同源CLI构建、tsc/Vite构建均exit0。
- 两安装实际CLI/stdin MCP：exit0，见actual-cli.json。未调用付费云API。
- GUI：45项，42 passed、0 failed、3 skipped，exit0；真实OS flag启用，导出/验证读取与安全存储建立/改密码/恢复GUI均实际执行。三项本地模型条件未启动不计通过。本轮320/1280截图已实际查看，七宽度不溢出，目录输入与触控按钮及成功/不导入说明可读。

原生Tauri窗口/picker本轮未操作。所有平台flush对象文件并在同文件系统rename发布，Unix另sync staging目录；Windows目录crash durability及本源码Windows/native五平台未实际验收。独立文件系统不等于S3/WebDAV/互联网同步；#35完整平台、性能、安装恢复门槛另行验收。未push、关闭Issue或触发CI计费。独占07 target，debug0/jobs2/incremental0；不修改实施者工作树或其他活跃cache。
