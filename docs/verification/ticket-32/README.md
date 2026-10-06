# #32 checkpoint — incomplete, not acceptance

Worktree `/tmp/wardrobe-ticket-32`, branch `codex/ticket-32`. Synthetic temporary installations only. No user database, person image, account key, push or ticket closure.

Implemented portable whitelist SQLite facts/media with explicit original option; actual prebackup and overwrite; readonly version/schema/hash/reference/content preview; restore reads a verified owned copy and excludes unlisted files; same-installation directory migration preserves local grants/config/wallet references and source. DB17 adds local settings and installation generation, both excluded portable; actual table repair coexists with accepted recommendations DB15 and insights DB14. Sync16 and weather18 remain unaccepted elsewhere and require accepted-module integration later.

## Confirmed terminal contracts

- `ticket32-content-decode-green.log`: 13 passed, extensionless transport display works for actual collage and thumbnail rebuild; prior missing-image red preserved.
- `ticket32-json-refs-green.log`: 14 passed; missing JSON asset reference was accepted despite valid hashes/FKs before its real red→green repair.
- `ticket32-inventory-green.log`: 15 passed; only listed verified objects are adopted.
- `ticket32-content-validation-green.log`: 16 passed; undecodable media with recalculated transport hash actual red→green.
- `ticket32-generation-green.log`: 17 passed, same-space old authorized Application waiting during actual prebackup copy cannot write after restore. Initial different-space test only produced incidental FK storage_error; same-space red actually returned successful unauthorized write and is preserved separately.
- `ticket32-18-green.log`: 18 passed, additionally malformed portable paths and future format/schema versions do not change current facts; subsequent old token is unauthorized.
- `ticket32-auth-continuation-seam-red-2.log`→`...green.log`: one deterministic **test-only authorization-phase seam**, not an actual timing reproduction. Restore between grant validation and execution previously silently reopened as trusted. `execute_current` keeps authorized context on original connection and refuses stale generation; write-lock acquisition rechecks generation.
- `ticket32-process-boundaries.json`: actual persistent MCP blocked writer, process kill during copy, and POSIX permission failure publishing marker retain the source. Not a Windows or power-loss claim.
- `ticket32-restore-client-process-green.log`: actual MCP restore blocked writer already green; original misleading filename renamed in evidence.
- `ticket32-gui-two-flows-green.log`: 2 passed, same CLI source as content/ref/generation/authorization continuation/path checks. Actual create/settings persistence, corrupt version preview refusal, overwrite/prebackup, seven widths and isolated actual directory switch plus old-path CLI/new GUI same data. Earlier second GUI failed only a locator matching four rendered elements; fixed exact heading selector.

## Regression still incomplete

`run-rust-regression.py` runs explicit workspace targets sequentially and deletes only its own just-terminated test executable to manage disk. Do not call this a single `cargo test --workspace` invocation.

First pass: library seam 1 passed, application 9 passed/1 failed because newly added operation descriptors omitted the established camelCase `inputSchema`/`outputSchema` and `schema_version` output contract. Source correction is included in this checkpoint, **not yet rebuilt/retested**. Remaining targets, fmt/clippy/build and complete GUI suite remain required.

## Capacity evidence boundaries

`scale.py` constructs the full 10k clothing/100k event + receipt/audit/outbox facts using public seed shapes expanded in an explicit synthetic SQL transaction. Nine valid synthetic PNG originals add >256 MiB aggregate media. All actual backup/preview/restore/count/restart/original reads use public CLI. No reduced counts.

First `ticket32-scale.log` failed with generic storage_error before operation phase attribution; low disk alone does not prove cause. Fresh whitelist DB no longer produces a full VACUUM temporary copy; canary test must remain green. Second `ticket32-scale-2.log` completed create/preview/restore and counts but final original read used wrong `assets.get` field `asset_id` instead of `id` (fixture failure, not product defect). Corrected full run is pending terminal when this file was drafted; final result will be recorded separately.

Still required: complete same-source CLI/MCP/GUI regression and member scopes, capacity terminal, restore rollback and interruption coverage, latest accepted integration merge, independent merger, and cross-platform/native final gates. #32 remains open. No claims of OS-wallet portability, inherited device sequence, paid cloud generation or whole Beta acceptance.

### Final result of this subturn

Session 40994 confirmed exit 1: full 743,403,520-byte synthetic source SQLite plus 270,001,539-byte media; actual create 19.339450s and preview 32.624948s passed, but `backup.restore` returned generic storage_error. TempDir cleanup completed and disk returned to 2.5 GiB. Cause is **unconfirmed**; do not attribute to source correctness or ENOSPC without specific evidence. `ticket32-scale-final.log` retains the actual failure. No capacity green claim.

All spawned terminal handles for this subturn have exited. No additional build was started after the root coordination request. This checkpoint releases the agent slot for required independent #16 reviews; #32 work resumes afterward with full original scope.


## Async maintenance checkpoint — incomplete

Full diagnostic scale run 74163 exited 0: 10,000 clothing, 100,000 wear events/receipts, 270,001,539 bytes original media and a 1,002,981,099-byte package. Actual create 13.653s, preview 17.272s, restore 39.602s, source removed before restore, restart/public counts/original reads passed. Earlier generic storage_error causes remain unconfirmed. This scale precedes async maintenance and stricter receipt identity validation; it is not final same-source capacity acceptance. The synthetic expansion now fixes each receipt request operation_key/clothing_ids to agree with its SQL row.

Public long operations now enqueue installation-local maintenance_jobs and immediately return a durable ID. Whole-space tasks have no member FK and cannot appear in generic member jobs APIs. GUI submits, drives a worker, polls state and exposes safe cancellation/explicit retry. Preview/domain validation now checks garment facts and SQL/document identities, historical roots/revisions, outfit version identities and original receipt revisions without rewriting immutable history.

- 65029: backup domain contracts 28 passed. Prior 51643 was 26/28 because two authorized test paths only enqueued without driving their worker; corrected explicit pipeline, not product failure.
- maintenance public contracts: 6 passed, including limited-member denial and generic jobs invisibility, durable queued cancellation/retry.
- 69948 lease cancellation deterministic seam: actual red (cancelled state could be overwritten by renewal); 95871 green after IMMEDIATE validation+renewal. A boundary simulation, not a claim of observed random production timing.
- CLI build 6659 and frontend build 56835 exited 0; GUI 60015 2 passed on that same CLI source, actual create/preview/overwrite/prebackup and old-path CLI after directory move.
- 57200 directory publication/worker-finalization boundary red; 45775 library 3 passed after querying a published move reconciles its receipt. Real copy + active marker, deterministic skipped final worker write, not a true process kill at that instant.
- 17151 **current red**: an expired maintenance receipt can be claimed even while a real local worker lock is held. Worker file-lock integration remains required; no final suite green claim.

Remaining: worker exclusion + crash/recovery receipts for all maintenance operations; final same-source CLI/MCP/GUI/full sequential Rust targets/clippy; real interrupted/cancelled process contracts; final full capacity; accepted root integration (weather explicit configuration only, no derived cache/errors); unknown provider submission + same-space restore cross-module gate; independent merger. No push or issue closure.

## Resumed implementation, current frozen source (not final acceptance)

Accepted weather DB18 is merged. Portable weather includes only explicitly configured settings; cache/errors are stripped before fresh whitelist inserts. DB17 local maintenance queue, generation and submission safety ledger remain excluded portable.

Actual process evidence now covers worker lock ownership, cancellation during copy, SIGKILL at controlled publication boundaries, and directory blocked writers. The controlled publication gates are enabled only by the debug `backup-test-diagnostics` feature; they prove specific crash boundaries, not random timing or power-loss resilience. Non-Unix file identity equivalence and Windows actual behavior remain unverified.

The real TCP submission/restore process first reproduced duplicate POSTs (provider_posts=2, first worker SIGKILL -9). The installation-local ledger now preserves submitted/unknown/received/cancelled safety receipts under the same restore transaction. The same script passes with provider_posts=1 and an explicit submission_already_sent refusal. Its matrix also checks changed payload conflict, member and actual workspace identity, queued-unsubmitted rebuild, completed cached immutable facts, rollback and portable ledger exclusion. No secret, photo or raw request is stored in this ledger.

Current async full capacity session 53135 confirmed exit 0 and automatic temporary installation cleanup: 10,000 clothing/100,000 wear records+receipts, source database 743,354,368 bytes, nine originals 270,001,539 bytes, package 1,002,927,851 bytes. Source installation removed before restore; restart/public counts and original file reads passed. Enqueue replies were 61.120/13.584/15.054ms; background create/preview/restore were 20.327/30.624/60.527s. **This precedes the following blob and directory repairs, so it is not final same-source capacity evidence.**

- 76273 real public negative: valid PNG replacement with old immutable blob ID and resigned transport hashes was accepted. Binding original/display ID and bytes to the verified inventory fixes it.
- 14571 then passed 29/30: concurrent old-connection directory test lost an acknowledged ID. This real regression is preserved, not dismissed as fixture noise.
- 79787 deterministic actual-file publication seam reproduced reading pending followed by active publication and then deleting the active marker. Resolver now re-reads the marker while holding the move lock and removes only orphaned pending records. 25768 library 9/9; 96575 backup 30/30. Actual CLI process 12598 also passed blocked writer, SIGKILL recovery and permission-failed publication.
- 77765 real public preview negative: a resigned weather document with embedded endpoint credentials was accepted. Existing configuration validation is now shared with portable document validation; 75342 backup 31/31 passed.

Full sequential Rust targets, final default CLI/MCP/GUI matrix, clippy/build and current-source full capacity are still underway. #32 remains incomplete and open until independent acceptance.


Latest sequential frozen business-source regression: session 56770 exited 0, 28 actual target commands, 216 passed / 0 failed / 12 ignored. Each target command and terminal executable cleanup is recorded in `rust/` and `ticket32-current-final-rust-targets.log`; ignored OS/model contracts are not claimed passed. Frontend 26096 and fmt check exited 0. Clippy initially found only three needless references introduced by extracting weather configuration; corrected and 25110 exited 0. Default CLI 98640 rebuilt after that lint correction. Actual TCP matrix 24583 exited 0 on this same default CLI, including an additional same-key changed-payload idempotency conflict without POST.

Current full GUI was already running when root requested temporary release for #17 independent review; it will finish naturally before checkpoint. No final capacity rerun has started. #17 feedback DB19 is not yet accepted and is not copied into this branch. Final source capacity and accepted feedback integration remain required.


GUI session 7634 confirmed exit 0: 47 passed, 6 skipped, 53 discovered, on the current default CLI. Includes both backup flows and seven-width checks. Skips are explicit real-resource opt-ins: two OS credential flows (WARDROBE_ENCRYPTION_OS_TEST not 1), cutout model download (WARDROBE_RUN_CUTOUT_GUI_SMOKE unset), SmolVLM provider (WARDROBE_REAL_LOCAL_PROVIDER unset), real recognition server (WARDROBE_RECOGNITION_REAL_ENDPOINT unset), and actual Qwen text server (WARDROBE_AI_ACTUAL_ENDPOINT unset). Test 42 is that explicit recognition endpoint condition. Feedback/sync unaccepted branches are absent, not tested here. This is #32 candidate regression, not all Beta acceptance.

All current handles have reached terminal. This clean checkpoint temporarily releases the agent slot for #17 independent two-axis review. Remaining full scope: accepted feedback/sync integration when available; final same-source full capacity after blob/marker/weather repairs; all three public entrypoints/member authorization final matrix and any resulting fixes; independent #32 acceptance. No capacity/build handle was killed.
