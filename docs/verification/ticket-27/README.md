# Ticket 27 implementation checkpoints (not final acceptance)

Issue: https://github.com/MingStudentSE/wardrobe-agent/issues/27 . All results below are terminal local evidence. This report does not claim native Tauri GUI, Windows, cloud transport, full Beta capacity or five-platform CI verification.

## Actual Application / two installations

The ignored OS-credential test is explicitly invoked with `--ignored --test-threads=1`. It creates two temporary SQLite installations, pairs through public APIs, uses generated images and removes temporary OS credentials on exit. A single evolving test covers historical snapshots, concurrency, encryption boundary and recovery; it does not replace a final full suite.

Evidence logs under `/tmp/wardrobe-implementation/`:

- `sync-conflict-red.log` -> `sync-conflict-green.log`: OR-set tags and scalar field conflicts. Green handle 8100 exit 0 (18.93s).
- `sync-same-choice-red.log` -> `sync-same-choice-green.log`: selecting the provisional scalar value still sends an explicit causal decision. Green 64451 exit 0 (19.89s).
- `sync-branch-choice-red.log` -> `sync-branch-choice-green.log`: both same-number template snapshots and wear corrections survive; explicit choice appends version/revision 3. Green 56348 exit 0 (21.87s).
- `sync-care-branch-choice.log`: additional actual care branch selection, all previous documents retained, green 75951 exit 0 (23.04s).
- `sync-identity-quarantine-real-red.log` -> `sync-identity-quarantine-green.log`: authenticated sender attempts to change an existing stable wear ID. It is quarantined durably, original history survives, healthy updates continue. Green 50914 exit 0 (26.60s). Fixture-only compile/status/recovery API failures are separately retained; they are not product red tests. The independent sender uses only recovery materials created for this test, never user keys.
- `sync-schema-red.log` -> `sync-schema-final-green.log`: a valid AEAD envelope with invalid category does not establish a valid fact. Full source-outfit snapshots pass validation. The first schema implementation incorrectly rejected a full source snapshot; that actual failure remains in `sync-schema-green.log`. Final 14236 exit 0 (26.84s).
- `sync-delete-set-red.log`, `sync-delete-review-red.log` -> `sync-delete-review-green.log`: first implementation rejected legitimate lifecycle metadata; subsequent test exposed missing explicit deletion/edit review. Concurrent remove/add preserves the new tag, delete prevents resurrection, and human restoration converges. Final 37621 exit 0 (35.74s).
- `sync-pending-restart-real-red.log` -> `sync-pending-restart-green.log`: a member commit is actually withheld while its clothing log arrives. Pending dependency survives closing/reopening the receiving Application; restoring the commit drains bounded replay. Green 40059 exit 0 (39.52s). The initial shadowed TempDir compile failure is not product red evidence.
- `sync-legacy-first.log`: actual legacy DDL fixture retaining public-created wear/care asset refs and documents; reopening repairs physical tables and foreign keys. This was an initial green verification, not a claimed red/green cycle.
- Earlier adopted-media, missing-display and #15 joint evidence: `sync-assets-green.log`, `sync-missing-green.log`, `sync-recommendation-joint.log`. The generated noisy 1200px image is transmitted in encrypted 128KiB chunks; the new installation has no original, generates an actual WebP thumbnail, and can record wear while a chunk is unavailable.

## Storage and authorization

- `sync-storage-contract-red.log` -> `sync-storage-contract-green.log`: stable advisory lock rejects symlinked lock path; invalid page limit/cursor rejected; streaming interruption publishes no object. All three green.
- `sync-portable-key-red.log` -> `sync-portable-key-green.log`: portable drive-qualified/absolute/non-normal/control-character keys rejected. Four green on macOS. The Windows escape was identified by source review; no actual Windows execution is claimed.
- `sync-storage-concurrent-green.log`: five total tests including a held publisher lock and concurrent conditional delete. New version remains present; obsolete delete precondition fails. This new concurrency check started green.
- Unix parent directories are synced after publication rename/delete; Windows directory durability remains unverified, consistently with ADR0019.
- `sync-scope-red.log` -> `sync-scope-green.log`: member-limited clients cannot smuggle `member_id` into whole-space sync operations. `sync-scope-capabilities-green.log` additionally checks capability filtering.
- `sync-cli-mcp-scope.log`: actual CLI and MCP subprocesses enforce space scope and expose usable member conflict descriptors. 92030 exit 0 (one test, 0.14s). This is separate from the trusted-local peer CLI used by GUI verification.

## Browser GUI with actual Application bridge

- `sync-gui-entry-real-red.log`: cross-device sync entry absent.
- `sync-gui-entry-final-green.log`: first component exposed an actual startup race: empty-member keyed mount was replaced and closed the details panel. Fixed by mounting after member identity loads.
- `sync-gui-mounted-green.log`: manual target entry/error and all seven viewport widths passed. Initial wrong working-directory and missing dynamic library attempts are retained separately; not counted as UI product red.
- `sync-gui-two-install.log`: `WARDROBE_SYNC_OS_TEST=1`, two passed, zero skipped, 88995 exit 0 (8.5s). The browser saves a real target, reviews two real name candidates and commits a choice. A peer adopts a generated PNG; an actual encrypted chunk is moved away, GUI reports missing display, then restore/retry yields an original-free asset with a real WebP thumbnail.
- `conflicts-320.png` and `conflicts-1280.png` inspected directly. No horizontal overflow at 320/375/414/768/1024/1280/1440. Existing Atelier tokens and touch buttons retained.
- `sync-ui-build.log`: TypeScript/Vite build terminal exit 0. It predates the pending module change and is not a final same-source joint validation.

## Required remaining work

Not accepted yet: bounded journal publication and scalable remote indexing/cursors, causal completeness across reordered logs, multiple targets/revocation/epoch replay contracts, remaining schema/asset identity and repair contracts, full 10k clothing/100k event behavior, full authorized CLI/MCP synchronization, stale-read GUI member-switch regression, ADR0021/GLOSSARY, accepted root DB17/18 physical coexistence and portable weather projection if applicable, final merged same-source whole-workspace regression and independent merger.

Do not use the green small two-installation story as proof of all Issue27 or #35 gates.
