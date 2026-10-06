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
