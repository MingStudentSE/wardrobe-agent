# Issue tracker: GitHub

本项目的问题、任务和规格记录在
MingStudentSE/wardrobe-agent 的 GitHub Issues，使用 gh CLI 操作。
在仓库目录内执行命令，仓库从 git remote 推断。

## 常用操作

- 创建：gh issue create --title "..." --body-file <文件>
- 读取：gh issue view <编号> --comments
- 列表：gh issue list --state open --json number,title,body,labels,assignees
- 评论：gh issue comment <编号> --body-file <文件>
- 添加标签：gh issue edit <编号> --add-label "<标签>"
- 移除标签：gh issue edit <编号> --remove-label "<标签>"
- 认领：gh issue edit <编号> --add-assignee @me
- 关闭：gh issue close <编号>

多行内容保存为临时文件，通过 --body-file 提交。

技能要求“发布到 issue tracker”时，创建 GitHub issue。
技能要求“获取相关 ticket”时，读取 issue 正文、标签和评论。

## Pull requests as a triage surface

PRs as a request surface: no.

## 父子任务与依赖

优先使用 GitHub 原生 sub-issues 和 issue dependencies。
API 中使用 issue 的数据库 id，不要混用编号或 node_id。

如果原生功能不可用：
- 子任务正文开头写 Part of #<父任务编号>，
  父任务正文用任务列表链接子任务。
- 依赖任务正文开头写 Blocked by: #<编号>。
- 所有阻塞任务关闭后，任务才可执行。

## Wayfinding operations

- map 是带 wayfinder:map 标签的 issue，
  正文记录 Notes、Decisions-so-far、Fog。
- 子任务链接到 map，使用 wayfinder:<type> 标签；
  type 为 research、prototype、grilling 或 task。
- Frontier 为 map 中尚未关闭、无人认领且无开放阻塞的子任务；
  按 map 中的顺序选择。
- 认领时分配给执行者。
- 完成后评论记录结果、关闭任务，
  并在 map 的 Decisions-so-far 添加可追溯的结果链接。
