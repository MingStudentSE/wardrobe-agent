# Ticket 16 independent merger: acceptance withheld

Base `9a5698a`; candidate `663596941c9f84cd794bb2b65b9e58be106fd56d`; separate worktree `/tmp/wardrobe-merge-16`. No issue closure, integration merge or push. Independent review follows implement-spec step 5, code-review two independent axes, project Hallmark audit and public Application/CLI/MCP/GUI seams. Scope remains parent F01–F30; this is only ticket 16.

Independent execution against candidate business source:

- Workspace: **166 passed, 0 failed, 12 ignored**, actual terminal exit0.
- Focused weather/AI: **9 passed**, including real TCP concurrent connections, cancellation/lease and cache boundary tests.
- Explicit real temporary OS credentials: **6 encryption + 3 encrypted package tests passed**. An initial wrong test-target plural failed command selection and was corrected; not a product defect.
- Original 10,000 clothing / 100,000 effective wear fixture: **1 scale contract passed**; wall test 9.96s includes preparation. No new performance percentile or baseline hardware claim.
- Clippy all targets, fmt, CLI and frontend builds: passed.
- Full development Chrome GUI: **47 passed, 3 skipped**, with actual Qwen selection and saved outfit creating no wear; public weather case and all seven supported widths included. Current screenshots inspected. Three skipped paid/local-recognition gates do not count as success.
- Actual public weather: Shanghai synthetic coordinates 31.23/121.47, **13.6°C**, retrieved 2026-10-06T20:58:47Z, model valid 20:45Z, attribution visible. Real Qwen: seven synthetic garments, one fixed coat, five legal candidates, **one POST, text_model, four legal items, zero wear**. Weak generated explanation remains visibly unverified.
- Actual limited CLI/stdin MCP entry script passes generation/get/list/replace/change/save, idempotent save and no wear.
- Actual CLI killed after server POST then restarted: **one total POST**, generation_waiting_input, explicit offline alternative, no wear.

Initial real script failed because this reviewer omitted native dylib environment, then succeeded with sourced environment. Initial GUI setup referred to missing implementer node_modules, then used existing integration dependencies. Neither was product red. All above logs/JSON are copied to `independent/`; author logs were not counted as independent runs.

## Standards

Raw independent report `independent/merge16-standards.md`: **0 documented-standard violations**, **2 nonblocking heuristic smells** (overlapping request validation; claimed-task parameter clump). No new Hallmark design-system/hover/narrow-flow violations.

## Spec

Raw independent report `independent/merge16-spec.md`: **2 P2 blocking specification gaps**:

1. Confirmed HTTP429 skips bounded retry and Retry-After (§3.3). New public Application/TCP test `confirmed_rate_limit_waits_retry_after_then_returns_valid_model_result` actually fails: one POST, expected two, no second attempt despite confirmed rejection. `independent/merge16-rate-limit-red.log` records exit101. No product fix made here.
2. Network-bound recommendation.generate waits synchronously instead of immediately returning persistent job_id (parent common API and docs/02:92). Durable submitted guard alone does not satisfy immediate long-task return.

Root assigned both fixes to original implementer together to avoid conflicting retry/async edits. Candidate remains unaccepted despite passing existing tests. A repaired source requires independent re-review and rerun of relevant entry, crash, model, GUI, and regression contracts before closure.

All reviewer build/test handles terminal. Owned Qwen server PID24224 / handle84405 received TERM and handle confirmed exit0 after actual proof; weights retained. Temporary target/node_modules symlinks removed. `independent/merge16-cleanup.json` records 24 rebuildable project test executables deleted after terminal checks and no live lsof match, **2,619,100,592 bytes**; CLI/libraries/build/models/source/evidence and 07/09 caches preserved. Root cache released.
