# Ticket 16 — weather and validated text recommendation evidence

Implementation branch: `codex/ticket-16`. Native macOS, Rust/SQLite shared Application, CLI/stdin MCP and Chrome development GUI. Independent acceptance is pending; no issue closure or push is performed here.

The default rule planner remains available without weather/model service. Weather requires an explicitly configured location and noncommercial free-service selection. Manual temperature and occasion and off remain usable. A cache records obtained_at, provider valid_time, source and attribution; stale means strictly more than three hours. Model output is organized from shared planner candidate sets and checked again against current inventory; explanations are unverified and never update clothing facts.

## Evidence distinctions

- `weather-red.log` is the actual unsupported public command red before implementation. Weather tests use real local HTTP protocol responses and real persisted Application state; they are not claims of public API success.
- `ai-recent-red.log` records a real current rule-validation defect: a model repeated recent wear despite an available fresh set. Green rejects it and falls back through the shared planner.
- `ai-concurrent-red.log` proves two real public Application connections submitted the same operation twice before the persisted guard. The green TCP test blocks the first POST and verifies one request.
- `logs/ai-crash-red.json` / `logs/ai-crash-green.json` use an actual CLI child killed after the HTTP server receives POST, followed by a restarted CLI. Green observes one POST and generation_waiting_input, with a separately requested rule result and zero wear. This is real process/HTTP behavior, not a paid provider or model call.
- `lease-boundary.log` blocks an actual provider response, then cancels through public jobs.cancel or expires a lease at the clock seam. Neither publishes a late intent; generic retry and same-key resubmission remain blocked. The unit `publication_boundary` directly exercises the narrow post-commit/pre-job-finish seam: public cancellation makes job_finish fail, but a same-key public call returns the already persisted intent without looking up a nonexistent provider. This unit is not presented as a reproduced concurrent timing race.
- `actual-weather-*` records public Open-Meteo and real local model responses. Early Smol and Qwen invalid/cold-coat/wrong-ID attempts remain evidence of genuine fallback, not successful AI selection. Four-fixed schema output proves only legal protocol IDs; `actual-weather-qwen-candidates.json` has seven garments, one fixed coat and five shared-planner candidate sets, and records a genuine legal model selection. Final same-source evidence is recorded separately.
- `model-download.json` records the official Qwen source, Apache 2.0 license, 491400032 bytes and verified full SHA256. Public synthetic wardrobe only; temporary databases/providers are removed, no user keys or user wardrobe read, no paid API or subscription.

Fixture errors (GUI provider.save inherited an unsupported member_id, an accepted nonblocking socket, and a unit's unnormalized default alias) and test compilation errors are preserved as such, not counted as product red. Network/unknown submission is never relabeled a confirmed provider failure.

## Reproduction

Use native media environment `/tmp/wardrobe-implementation/ticket16-env.sh`, Cargo jobs 2, incremental 0, debug 0. Root cache is exclusive to this implementation during verification. Do not rebuild CLI while GUI is running.

- `cargo test --workspace`; explicitly run recommendation 10k/100k ignored scale test; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo build -p wardrobe-cli`.
- `python3 docs/verification/ticket-16/reproduce-crash.py /absolute/target/debug/wardrobe-cli`.
- `python3 docs/verification/ticket-16/reproduce-entry.py /absolute/target/debug/wardrobe-cli` exercises limited CLI and stdin MCP against shared constraints. Token exists only in temporary process memory and is omitted from output.
- Start the verified local Qwen GGUF with the existing llama-server, localhost 18086, alias wardrobe-qwen, context8192, CPU2. `WARDROBE_ACTUAL_MODEL=wardrobe-qwen python3 docs/verification/ticket-16/reproduce-actual.py /absolute/target/debug/wardrobe-cli` captures public weather and raw actual model request/response. Stop the server afterwards.
- Frontend build and Playwright: `WARDROBE_ENCRYPTION_OS_TEST=1 WARDROBE_AI_ACTUAL_ENDPOINT=http://127.0.0.1:18086/v1 WARDROBE_TEST_PORT=5187 npm run test:e2e`. Real actual-model test is gated by the explicit endpoint; a skip is not a pass.

## Limits

This verifies a small local model's legal-ID selection, not general styling accuracy. Its recorded explanation can be weak or inaccurate and stays marked unverified. The free weather endpoint is an authorized noncommercial example, not a purchased commercial service. Native Tauri GUI, other OS execution, paid cloud models and full Beta/#35 platform gates are not claimed. Generic compatible chat has no outcome query; crash/unknown outcomes require provider inspection or an explicit new offline intent, never blind same-key paid retries.

## Final implementation verification

All checks below used the business source now in this candidate after merging root `9a5698a`:

| Check | Actual terminal result |
| --- | --- |
| Workspace Rust | 166 passed, 0 failed, 12 ignored; `logs/final-workspace.log` |
| Full original scale | 10,000 clothing, 100,000 effective wears, 5/5 results, fixed preserved, 4516 ms, DB 159842304 bytes; `logs/final-scale.log` |
| Clippy, fmt, CLI build | Passed; `logs/final-clippy.log`, `logs/final-cli-build.log` |
| Actual OS temporary-key tests | 6 encryption + 3 encrypted packages passed separately; `logs/final-os.log` |
| Frontend build | Passed; `logs/final-ui-build.log` |
| Full Chrome GUI | 47 passed, 3 explicitly skipped; `logs/final-gui.log`; actual Qwen model case passed |
| Actual public weather + model | `final-actual.json`: Shanghai public coordinates, 13.6°C, real retrieval/valid timestamps; actual Qwen one request, seven garments, one fixed coat, five planner candidates, legal four-item result, wear0 |
| Actual limited CLI/stdin MCP | `final-entry.json`: weather configure/refresh/get and recommendation generate/get/list/replace/change/save, persisted save idempotency, one template, wear0; model connection failure gives rules |
| Actual CLI kill/restart | `final-crash.json`: one observed POST, restarted same-key returns generation_waiting_input, explicit offline request works, wear0 |

After the complete regression, only the cancellation test fixture was expanded to include an invalid first response before cancellation; product code stayed identical. `logs/final-submission-boundary.log` reruns all six AI public contracts and confirms submission_count stays one (no second invalid-output submission after cancellation), expiry/publication guards, shared availability validation and same-key cache. `logs/final-test-clippy.log` checks this final test source. The ordinary two-confirmed-invalid-response test still proves exactly two bounded submissions and rule fallback.

Inspected new `weather-ai-320.png`, `weather-ai-1280.png` and `actual-ai-gui.png`. Seven automated widths have no horizontal overflow. GUI displays attribution, obtained time, manual temperature, model fallback and unverified explanation; actual model selection is saved without a wear event. Other tests' regenerated baseline screenshots were restored, so this candidate contains only ticket 16 screenshots.

`cache-cleanup.json` records 22 confirmed terminal project test executables removed (2400219344 bytes), no live lsof matches, preserving current CLI/hash executables, libraries/build/model/evidence and 07/09 caches; actual available blocks rose from2435940KiB to4781800KiB. Cleanup is not a verification failure.

The local Qwen server remains available **at the parent's explicit handoff request**, localhost18086, PID10390 / original tool handle22569. Its weight SHA is in model-download.json; alias wardrobe-qwen, context8192, parallel1, CPU2. Independent merger owns the subsequent shutdown; do not run another server on that port. The implementation's Cargo/Playwright handles are terminal. No user key, actual wardrobe or paid cloud call was used. Root cache is released after candidate handoff.

Implementation review of the four issue criteria: actual weather/model demonstration, shared current-inventory constraints with bounded fallback, offline/manual three-entry availability, and reproducible public tests/ADR/glossary are demonstrated here; **independent merger acceptance remains pending**. General model explanation quality, native Tauri execution, other OS, paid providers, full sync of new weather configuration and full Beta/#35 gates are not claimed.
