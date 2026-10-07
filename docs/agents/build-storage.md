# 构建必须位于备份盘

2026-10-07 用户确认：完整 macOS Beta 继续开发；本项目所有后续构建必须在 1TB「备份」盘执行，禁止回落系统盘。

物理存储为 `/Volumes/备份/CodexStorage/CodexWorkspaceHFS.sparsebundle`。备份盘为 ExFAT；映像内部使用 Mac OS 扩展日志式文件系统，保留 Git/Node 的符号链接、权限和可执行属性，不格式化备份盘。映像最大逻辑容量 200 GiB，按实际写入增长。

映像挂载于 `/Users/links/.codex/external-workspace-volume`。该路径虽然位于用户目录，数据实际落在备份盘。原 `/Users/links/.codex/wardrobe-execution` 和 `/Users/links/.codex/worktrees` 均通过符号链接指向映像内的 `codex/` 目录，原有 Git 路径保留。

## 执行规则

1. 执行 `/Users/links/.codex/mount-codex-backup.sh`。备份盘缺失或映像挂载失败必须停止。
2. 构建前验证实际工作区路径位于挂载盘，使用 `pwd -P` 和 `df -h`。不要在其他本机临时目录构建。
3. Rust 每次 shell 调用都 source `/Users/links/.codex/wardrobe-execution/native-env.sh`；它检查挂载和工作区，设置外盘 TMPDIR、npm cache 与默认 Cargo target。
4. 各并发任务使用独占外盘 `CARGO_TARGET_DIR`，例如原路径别名 `/Users/links/.codex/wardrobe-execution/cache/<任务名>`。设置自定义值时须验证其真实路径在外盘；不共享并发写入缓存。
5. React/Tauri 的 node_modules、dist、target、打包产物和临时文件都放在外盘工作树或外盘缓存。系统已有 Rust/Node 工具链和已安装应用无需迁移。
6. GLM 开发使用用户当前指定的 zcode CLI / GLM-5.3-Flash；会话先验证真实 model_usage。TUI 在外盘工作区启动并设置外盘临时目录；原生构建每次 source 外盘 native-env.sh。`run-zcode-task.py` 提供带挂载、模型与路径检查的会话恢复入口；若当前 headless 无权限客户端，使用 TUI 审批已授权的开发命令，不把未执行命令算作成功。Unix socket 需要短 TMPDIR 别名，别名也必须指向外盘。
7. 开发、测试、打包期间保持备份盘连接。卸载映像或拔盘前停止全部写入任务。重新挂载后继续使用原路径。

## 验证与恢复

迁移证据位于 `/Users/links/.codex/storage-migration-20261007/`，包括逐文件 SHA256、Git HEAD/未提交 diff/索引状态和符号链接核对。构建遇到未挂载错误时重新运行挂载脚本；禁止在空挂载点下新建本机替代目录。源码/未提交内容不是可清理缓存。
