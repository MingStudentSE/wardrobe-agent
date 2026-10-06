# #17 explicit recommendation feedback — author candidate verification

Source branch codex/ticket-17 began at accepted integration d813dba. Prerequisites9/15 CLOSED live checked before starting; no independent acceptance or issue closure claimed.

Public seams are the shared Application API with real persisted SQLite, actual CLI/stdin MCP with temporary limited authorization, and GUI through the same-source CLI. Feedback opinions/scores are separate from explicit real wear; no user wardrobe, real credential or inferred preference profile is used.

## Red / green evidence

- first-red.log: feedback.record unavailable; first-green.log: optional scores/null, durable read/list and same-key idempotency, zero wear.
- day-red.log → day-green.log: explicit same-local-day garment exclusion, expiry provenance and fixed constraint conflict, zero wear.
- actual-red.log → actual-green.log: confirmed actual/changed feedback uses shared wear writer in the same transaction; exactly one actual wear on retry, shared wear regressions8 pass.
- history-red.log → history-green.log: appended correct/revoke, stale revision rejection, complete paginated history, original proposal unchanged.
- another-red.log → another-green.log: skip plus shared immutable change, fixed preserved, exact disliked combination excluded from future plans, zero wear.
- asset-red.log: detach downgraded adopted history display to preview. asset-green.log preserves selected display and feedback snapshots after detach/archive/delete; originals remain excluded by existing manifest whitelist.
- late-feedback-contract.log: real HTTP worker blocked during POST; new public day exclusion of fixed garment rejects late result, failed job/constraint_conflict/result null and no new intent. Initial fixture incorrectly expected worker Err instead of its documented failed-job return; late-feedback-fixture-error.log preserves that expectation error, not a product red.
- public-contracts.log: nine feedback public tests passed, including limited scope/member boundaries, score validation, pagination, schema19 missing-table repair and concurrent revision CAS.
- gui-red.log: feedback controls absent. gui-green.log: first opinion/score/skip/history/seven-width slice passed. Expanded actual/changed wear, day exclusion, correction/revocation and response-loss retry were subsequently exercised through real GUI controls.

history-compile-error.log retains a digest formatting compile error; clippy-initial.log retains a meaningless constant assertion removed from the test; gui-missing-native-env.log retains a missing native dylib environment invocation. These are implementation/tooling errors, not domain behavior red.

## Boundaries

Inventory snapshots are explicitly observed at feedback-record time, not falsely labeled the earlier recommendation-generation clothing facts. Original intent/proposal roles/IDs remain immutable. Feedback revocation never revokes real wear; wear history separately corrects/revokes actual facts. Dislike excludes precisely a combination, not every garment/style; day exclusion ends at the saved IANA local-day boundary. Try Another's durable skip may precede a failed/deferred change, which is clearly reported and deduplicated.

DB19 adds feedback_events, feedback_receipts, feedback_asset_refs with unconditional missing-table creation and shared max version. Future backup/sync needs the explicit new business fact/ref allowlist; no full transport or learning implementation is claimed here. No paid model, other OS/native Tauri or fullBeta/platform gate is claimed. Independent acceptance remains pending.


## Final author verification

All commands use the native environment recorded in native-environment.json, j2, incremental0 and debug0. Latest integration90b8a18 is included. These are author results, not independent acceptance.

- logs/workspace-final.log:180 passed,0 failed,12 ignored. Ignored tests are not counted as passes.
- logs/clippy-final.log and cli-final-build.log: exit0; actual-entry.json/reproduce-entry.py exercise the same-source CLI and stdin MCP with temporary limited scopes, immutable history and exactly two explicitly confirmed wear events.
- logs/os-final.log:6 actual temporary system-secret/encryption tests plus3 actual encrypted-package tests passed, separately from the default suite.
- logs/scale-final.log:10,000 garments/100,000 effective wears,5 requested/5 returned, fixed preserved,4368ms; temporary database159875072 bytes, cleaned. One sample is not platform gate35 p95 evidence.
- logs/gui-final.log:51 passed/3 skipped, total54,1.9m. Real local Qwen/weather regression and late-response cancellation passed. The final neutral TryAnother notification copy is additionally verified by gui-copy-final.log; it reports saved skip and points to actual task/result state rather than claiming a canceled child succeeded.

Criterion1: public persisted opinions/day exclusions and optional integer scores, correction/CAS and full paginated history; GUI controls, reload and seven widths.
Criterion2: ordinary feedback leaves wear0; actual/changed confirmation uses the shared atomic wear writer, CLI/MCP retries and real UI503 response-loss retry retain one receipt/one wear. Explicit changed confirmation adds the second wear. Feedback revocation does not revoke wear.
Criterion3: shared TryAnother preserves fixed/excluded roles and immutable parent; disliked exact sets and active local-day exclusions enter the planner and fresh publication validation. Real blocked TCP response after day exclusion fails without publishing an excluded intent. Snapshots and selected adopted displays survive detach/archive/delete.
Criterion4:9 public feedback contracts, full integration suite, actual three-entry evidence, ADR0024/GLOSSARY and reproducible evidence; no placeholder learning.

The first full GUI run (gui-full-fixture-pollution.log) ended46 passed/5 failed/3 skipped. The new feedback tests polluted the shared default member with garments/wears, changing member/article and pagination counts. They now create a separate member through UI and return to 我 in afterEach; unchanged baseline member/pagination assertions all pass. first-full-failure retains raw contexts. gui-isolation-fixture-error.log records waiting only for a member option, before selection settled; the fixture now waits for selected canonical ID.

gui-reexclude-red.log queried before explicitly awaiting the second receipt and is not the definitive red. gui-reexclude-receipt-red.log waits for acknowledged revocation and full subsequent record response: broken cache behavior returned the old revoked root. The final GUI proves a new root after revoke while preserving the original operation key for unknown503 retries. Only confirmed keys associated with a successfully changed/revoked root are cleared.

Owned local Qwen PID54120/session38273 remains available on127.0.0.1:18086 for independent review, using existing public model weights and synthetic inventory. No credentials are stored in evidence. No paid cloud accuracy, native Tauri, other platform, full sync/backup feedback serialization or fullBeta gate is claimed.
