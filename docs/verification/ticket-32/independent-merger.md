# Ticket 32 independent merger acceptance

Accepted implementation candidate `1451d47db771f3004feef76889fe1f9ca3e7ecd6`, independently checked in `/tmp/wardrobe-merge-32`, branch `codex/merge-32`, against accepted integration `1bd77377519176a2dd63c2b9f6667c28a410f6bd`. The integration is an ancestor. No product repair was needed. Author green logs were read as context, never counted as independent execution. This report and the adjacent `independent/` and `independent-rust/` directories are merger evidence.

## Standards

A separate Standards agent reviewed the fixed diff, project instructions, domain/ADR rules, TDD and pinned Hallmark requirements. **0 documented standards violations; 2 nonblocking judgement smells**: broad backup orchestration module and repeated filesystem helpers. Original, uncombined output: [Standards axis](independent/merge32-standards.md).

## Spec

A separate Spec agent reviewed the same fixed diff against live issue32/parent1, F22/F23 and ADR0022. **0 actionable Spec findings**. Original output: [Spec axis](independent/merge32-spec.md). Both agents completed independently in parallel without builds; runtime conclusions below come from this merger.

## Actual independent execution

| Check | Actual result | Evidence |
| --- | --- | --- |
| All current Rust targets, sequential builds | 29 unique commands, 230 passed, 0 failed, 12 opt-in ignored | `independent-rust/wardrobe-cli-summary.json` is cumulative and authoritative; do not add the core summary a second time |
| Real macOS encryption credential/package opt-in | 6 + 3 passed | `independent/merge32-os.log` |
| Workspace clippy, all targets, warnings denied; fmt | Both passed | `independent/merge32-clippy.log`, `merge32-fmt.log` |
| Desktop frontend build | Passed | `independent/merge32-ui-build.log` |
| Actual CLI/MCP authorization and revocation | Passed | `independent/merge32-authorization.json` |
| Five transported public feedback packages | Four actual invalid previews rejected; one legal nonconsecutive earlier predecessor accepted | `independent/merge32-feedback-audit.json` |
| Real TCP POST/submission restoration matrix | Passed | `independent/merge32-submission-matrix.jsonl` |
| Directory process/permission boundaries | Passed | `independent/merge32-directory-process.json` |
| Restore versus a real blocked authorized writer | Passed | `independent/merge32-restore-client.json` |
| Diagnostic phase SIGKILL and natural running cancellation | Passed | `independent/merge32-maintenance-process.json`; diagnostic build log and restored default CLI build retained |
| Full GUI with OS/model opt-ins | First full command: 53 passed, 3 skipped, 1 model-resource failure; affected real-model focused rerun: 1 passed | `independent/merge32-gui.log`, `merge32-qwen-retest.log`; **54 unique passing cases / 3 explicit skips**, not one fully green command |
| Full capacity backup/preview/restore | Passed | `independent/merge32-fullcapacity.log` |

All commands were run against candidate source, with exclusive `/tmp/wardrobe-implementation/ticket-09/target`, native environment from `/tmp/wardrobe-implementation/ticket16-env.sh` overridden to that cache. Full Rust runner built one target at a time and removed only terminal project test executables after checking no open reference. The diagnostic CLI was separately copied for exact gated process tests and the default CLI was rebuilt before public scripts/GUI. No other integration cache was modified.

Actual tool handles: Rust52879, diagnostic/default build84390, frontend42474, public scripts70690, OS/clippy/fmt13674, fullGUI22547, fullcapacity43324 and focusedmodel33596 all polled terminal. All exit0 except original GUI22547 exit1, recorded below. Initial root-ref shorthand and frontend command at repository root were corrected invocation errors, not product regressions.

### Contract boundaries actually exercised

Member-restricted CLI and MCP grants cannot create/preview/restore/move or control global maintenance jobs; generic job reads do not expose them. Every job action retains its original business scope. Whole-space MCP create/preview/restore/move succeeds; restore revokes old grants and subsequent CLI/MCP resolves the current directory.

Feedback corruption cases use valid transport checksums/FKs and actual public preview: illegal initial predecessor, missing predecessor, cross-member predecessor and cross-member display reference all fail. A legal revision3 pointing to earlier revision1 succeeds, preserving actual historical roots and display references rather than imposing consecutive numbering.

The TCP fixture kills an actual worker after POST. Same-space/member/key cannot blindly send again after old portable restore, unknown outcome, cancellation or rollback. Changed payload conflicts; a queued unsubmitted intent may explicitly rebuild. A completed fact already in the package returns its real immutable document and intent receipt without an invented job or new POST. Another member and an explicitly distinct workspace with the same member UUID/key are isolated. Private guard tables/hashes do not enter the portable package.

Directory migration was exercised against a persistent blocked MCP writer, an actual SIGKILL during copy, and chmod publication failure. Source remains recoverable until verified publication; restart/retry resolves the committed outcome. Restore blocks an already authorized writer and rejects its stale generation. Diagnostic phase gates target package rename-before-receipt, restore commit-before-worker-finish and directory publication-before-finish; these are deterministic actual SIGKILLs, not random power failure certification. Natural running cancellation waits for real media progress and yields no final package or resurrection.

### Full capacity and originals

The real temporary fixture contained **10,000 clothes, 100,000 wear events**, receipts/audit/outbox, **9 PNG originals / 270,001,539 bytes**, and feedback corrections, revocations, observations and historical display assets. Its source database was **743,591,936 bytes**. Fixture seeding expands public API shapes in an explicit synthetic SQL transaction; this does not claim 100,000 individual public writes. Backup/preview/restore, public reads and restart are genuine CLI execution.

The source directory was removed before restoring the package into another target. Actual final package **1,003,071,211 bytes**. All original identities/bytes, counts, feedback histories and observation facts survived and subsequent restart/public reads passed. Create **19.198s**, preview **39.472s**, restore **57.748s**; immediate durable receipt replies **12.626ms / 9.796ms / 13.547ms**. These are one measured local run, not p95 or a device-wide performance promise. Temporary capacity directories were removed by the successful script.

### GUI and actual model resource incident

The full suite retained actual delayed member-selection acknowledgement isolation, numeric/date/correction/revocation insight assertions, seven-width backup UI, explicit restore confirmation, historical feedback, late-response cancellation and real OS opt-ins. Backup screenshots in this ticket were independently regenerated and inspected; the 320px flow retains readable scope, confirmation, task query and status controls without hover or horizontal critical paths.

During real-model case56, localhost18086 was unreachable (actual health curl exit7). The original model owner polled16553 and received Unknown process id, with PID97691 absent and no listener; its exit code is unavailable, and is not called exit0. The complete suite ended 53passed/3skipped/1failed; the actual timeout is retained. Once the owner confirmed absence, this merger started the same existing licensed Qwen in owned handle2873, verified health, then independently reran only the affected case: **1passed, 19.7s**, real public weather, five legal planner candidates, fixed coat, actual text-model result, explicit save and unchanged wear facts. Other53 already passing cases were not repeated because source was unchanged. The fresh local model SHA256 is `74a4da8c9fdbcd15bd1f6d01d621410d31c6fc00986f5eb687824e7b93d7a9db`, matching prior official Apache2.0 evidence; see `merge32-model-sha.log` and actual server log. No model download, user key, paid call or user wardrobe was used.

## Accepted scope and limits

Portable schema19 includes the accepted feedback history/receipts/display references and weather18 configuration, excluding weather cache/errors, providers, credentials, encryption trust, client grants, maintenance records and local submission guards. Same-device directory migration preserves required private installation state. Later unaccepted learning20/sync16 joint serialization is not certified by this candidate. Native Windows/Android/Tauri and random hardware/power failure are outside this run; explicit opt-in GUI skips remain skips. This is ticket32 acceptance on the current accepted business model, not full Beta completion.

No root merge, push or issue closure was performed by the merger. Root retains controlled final integration/issue acceptance.

All HTTP tests were terminal before the uniquely identified owned Qwen PID received SIGTERM. Owned server handle2873 was polled **terminal exit0**; stop identity is in `independent/merge32-model-stop.json`. Model files remain. Original owner16553 missing exit status remains explicitly unavailable. Capacity and cache exclusivity were released to root after completion.
