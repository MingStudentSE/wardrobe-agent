# Domain Docs

## 探索代码前

读取根目录 GLOSSARY.md，以及 docs/adr/ 中与当前工作相关的 ADR。

这些文件尚不存在时，直接继续工作。
由 domain-modeling 技能在术语或决策明确后按需创建。

## 文档布局

采用 single-context：
- GLOSSARY.md：项目统一领域术语。
- docs/adr/：架构决策记录。

## 使用统一术语

Issue 标题、设计建议、假设和测试名称中的领域概念，
应使用 GLOSSARY.md 定义的词汇，避免使用其明确排除的同义词。

遇到未定义概念时，先检查是否需要该概念；
确有缺口则留给 domain-modeling 处理。

## ADR 冲突

建议与现有 ADR 冲突时，明确指出对应 ADR 及重新讨论的理由，
不要静默覆盖已有决策。
