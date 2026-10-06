Independent Spec-axis review: ticket #17, candidate 2e027886dcf537bf7e039e5a956205a19ff01d3b against fixed baseline 90b8a18. Read actual issue #17 (no comments), parent #1, docs/05 F11/F30 and §3.3, ADR0024, glossary and implementation/test source. No edits or builds; author logs do not establish independent runtime acceptance.

0 actionable Spec findings.

Issue #17: “喜欢/不喜欢/跳过/当天排除分别记录；总体/舒适/风格为可选1–5整数.” feedback.rs:335–358 preserves the separate reactions and initializes scores to null, rejecting nonintegers/out-of-range values. Day exclusion is explicit selected proposal IDs with IANA date/expiry (:385–422), and never changes lifecycle or wash state.

Issue #17: “实际穿了/改穿进入显式穿着流程，其他反馈不增加次数；同操作重试去重.” feedback.rs:425–484 requires confirmed=true and distinguishes original from explicitly changed combinations. It delegates to the existing wear transaction writer; feedback, wear and receipt commit together (:477–502). Ordinary feedback rejects wear input. Authorization separately requires wear.record (authorization.rs:124–140), preventing compound-operation scope expansion.

Issue #17: “Try Another依据约束和反馈改变候选，历史可读供后续学习.” feedback.try_another records skip and calls existing recommendation.change with stable derived keys (feedback.rs:211–226). The planner reads effective day exclusions and disliked exact sets inside its IMMEDIATE transaction (recommendations.rs:277–302, :398); current validation and lease checks remain on publication (:590–599), including late HTTP returns. History/revisions are paginated and member-scoped. Failed changing clearly preserves the already committed skip rather than implying atomic completion.

Feedback captures the immutable proposal plus explicitly labeled inventory_observed_at at feedback time (feedback.rs:486–493), not invented generation-time facts. Historical adopted display references persist across revisions, detachment and sync-manifest selection (feedback.rs:161; assets.rs:64–78, :140), retaining the existing asset whitelist.

Correction/revocation is a reasonable supporting scope, preserving historical audit and distinguishing feedback from real wear. Learning reconstruction, full transport/backup integration and native platform gates remain later full-Beta obligations, not claimed completed here. Actual three-entry operation, concurrency, image retention and GUI behavior remain the parent merger’s independent verification responsibility.
