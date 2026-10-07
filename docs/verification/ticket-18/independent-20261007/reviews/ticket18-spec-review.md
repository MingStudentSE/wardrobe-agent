# Ticket 18 Spec-Axis Review (independent, read-only)

Worktree `/Users/links/.codex/wardrobe-execution/worktrees/merge-18`; HEAD verified `7a4e3a21a2230ab9c6ddcb1cfee7172abd95f10b`; diff `b6ee5c8...HEAD` nonempty (127 files, +9546). No mutations/tests/runs performed; scope inspected from source at pinned SHA.

## Requirements verified present (hard evidence)

- **Explicit vs inferred, with source + algorithm version** — `learning.rs:5` `ALGORITHM="explicit-and-events-v1"`; explicit `source.kind="explicit_user"` (`learning.rs:762`), inferred `source.kind="inferred_events"` with feedback/wear IDs (`learning.rs:539`). Spec: “显式偏好与推断画像区分且有来源、算法版本”。
- **View/edit/delete/reset** — ops `learning.get/set/edit/delete/reset/history` (`learning.rs:599–785`); GUI view/edit/delete/reset/history incl. job cancel (`LearningMemory.tsx:20–39`). Spec: “用户可查看编辑删除与重置”； parent story 45.
- **Reset retains wear facts; reproducible recompute** — recompute only reads `feedback_events`/`effective_wears`, writes only learning tables (`learning.rs:498–538`); tests `learning.rs:36,140`. Spec: “重置不删穿着事实”.
- **No inference promoted to body dimensions** — dimension whitelist rejects size: “偏好维度不支持尺寸或事实” (`learning.rs:710–717`); absorb extracts only 6 attribute dimensions (`learning.rs:460–475`).
- **Recommendation reads learning** — `learning_weights` → ranking sort key + `learning_context` in public doc (`recommendations.rs` diff); test `learning.rs:102`.
- **Member isolation** — cursor-ownership check (`learning.rs:628–636`), portable member checks (`learning.rs:828–847`); test `learning.rs:317`.
- **Three-entry parity** — single dispatch `lib.rs:314`, descriptors `lib.rs:380`; CLI/MCP share `app.execute/execute_as_client` (`wardrobe-cli/src/main.rs:87–171`).
- **Durable job guards** — enqueue stores `expected_state` (`learning.rs:276–284`); publish re-checks lease token/expiry → `lease_lost`, and state+source version → `memory_changed` (`learning.rs:544–559`); cancel survives reopen, running worker can't publish after reset/cancel, worker-only client can't lift boundary (`learning_jobs.rs:62,195,268`).
- **Generation guard** — installation generation table (`learning.rs:9–19,350`), `source_stale` at read drops stale inference from ranking (`learning.rs:680–686`, `:227`); portable export strips `local_source_version/local_installation_generation` (`backup.rs` `strip_learning_local_epoch`), foreign inference marked stale (`learning_jobs.rs:125–190`).
- **Semantic timestamp fix** — cutoff compares parsed RFC3339 instants, not strings (`learning.rs:434–449`); +14:00-offset regression for wear and feedback (`learning_reset_offsets.rs`).
- **Portable projection / public archive regression** — 5 learning tables in backup TABLES with typed validation + receipts (`backup.rs` diff, `learning.rs:802–1033`); typed whitelisted `learning_doc` in device package (`package.rs` diff); db19 archives restore (`learning_jobs.rs:430`); scoped clients need `learning.recompute` for worker/cancel/retry (`authorization.rs` diff).

## Minor observations (not spec breaches)

- `learning.edit` rewrites `created_at` per revision (`learning.rs:112`); the field acts as revision time, not creation time.
- `learning.delete` permanently suppresses the feature until reset/rebuild (`learning.rs:132–134`) — beyond literal “删除记忆”, but disclosed in GUI copy.

Whole-Beta scope (three-platform installers, native/CI) remains pending and is outside this issue; issue-local behavior matches spec wording.
