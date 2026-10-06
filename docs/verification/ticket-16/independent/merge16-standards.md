Hallmark critique: P4 H4 E4 S4 R5 V4.

Standards axis, fixed point 9a5698a; candidate 663596941c9f84cd794bb2b65b9e58be106fd56d. Reviewed source diff and commit log independently; read AGENTS.md, domain rules, UI standard, design.md, GLOSSARY, ADRs 0001/0002/0008/0020/0023, pinned Hallmark audit rules and 320px screenshot. No builds or source edits.

Documented standard violations: none found. New functionality remains behind the shared Application API and descriptors, has persisted weather and durable submitted jobs, strips internal lease tokens from published intents, distinguishes model explanation from clothing facts, and preserves separate actual-wear semantics. UI extends existing Atelier classes/tokens with native controls, visible labels, status/error text and click-accessible details; no added hover-only or desktop-width-dependent critical path. Narrow screenshot is supporting visual evidence only, not native desktop or Android acceptance. Existing project visual-system rules override Hallmark theme/macrostructure rotation.

Judgement smells (nonblocking):
1. Duplicated Code / Shotgun Surgery: recommendations.rs:58 and :1082 maintain overlapping generation field lists, with key/timezone/limit validation repeated again in ai_recommendations.rs:69. A future control can become supported in one route but rejected in another. Prefer one authoritative generation-request validator used by both persisted and preview paths; keep operation-specific lists distinct.
2. Data Clumps: ai_recommendations.rs:312 text_recommendation_request passes member, job, lease and attempt together, while text_submitted/text_outcome separately pass overlapping identity fields. A small private claimed-text-task struct could encode this identity and reduce accidental mismatches if another text workflow appears. Current scope is small; no generic provider/task framework warranted.

Neither smell establishes a present standards violation or justifies blocking acceptance by itself. Functional correctness and specification coverage are separate review axes.
