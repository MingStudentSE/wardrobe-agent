# wardrobe-agent

## Agent skills

### Issue tracker

需求、规格和任务使用 GitHub Issues。
参见 `docs/agents/issue-tracker.md`。

### Triage labels

使用五个默认 triage 标签。
参见 `docs/agents/triage-labels.md`。

### Domain docs

采用 single-context：根目录 GLOSSARY.md 和 docs/adr/。
参见 `docs/agents/domain.md`。

### UI design

UI 设计、实现、审查与重设计使用 `ui-ux-pro-max` 技能。以用户指定的 `ui-ux-pro-max` 技能为准；参见 `docs/04-UI设计与跨端适配.md`。Beta 只交付桌面端，但共享视觉语言和组件必须兼顾后续 Android；不得依赖 hover、拖拽或桌面窗口宽度完成关键流程。既有产品要求优先于技能默认值，已有视觉系统建立后保持全应用一致。

### Build storage

所有构建、缓存与构建临时文件必须在 1TB 备份盘运行；先检查挂载，不得回落系统盘。参见 `docs/agents/build-storage.md`。

### Data directory isolation

数据库 schema 迁移只向前、不可逆。开发构建、测试运行和 GUI dev 会话必须使用隔离的 `WARDROBE_DATA_DIR`（source `native-env.sh` 自动获得），禁止用默认数据目录打开数据库；违者会把生产库迁移到未发布版本，导致正式应用无法打开。参见 `docs/agents/build-storage.md` 第 8 条。

### Branch model

双分支模型(2026-10-09):`master` 是用户发布线(默认分支),`development` 是开发集成线。功能开发与验收在 development 完成,统一并验收后随发布提升到 master;不直接向 master 提交功能。历史分支 main、codex/beta-integration(-continue) 已并入这两条线并删除;`codex/fix-40-backup-pair-projection` 有未合入内容,处置归 #66。
