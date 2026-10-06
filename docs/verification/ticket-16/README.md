# Ticket 16 — weather and validated text recommendation evidence

Implementation branch: `codex/ticket-16`. Native macOS, Rust/SQLite shared Application, CLI/stdin MCP and Chrome development GUI. The initial candidate was independently rejected for missing Retry-After and immediate task receipts; a repaired candidate is being verified below. Independent acceptance is pending; no issue closure or push is performed here.

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

## Historical first-candidate reproduction

These historical synchronous scripts apply to commit6635969 only. Use the async scripts below for the repaired API.

Use native media environment `/tmp/wardrobe-implementation/ticket16-env.sh`, Cargo jobs 2, incremental 0, debug 0. Root cache is exclusive to this implementation during verification. Do not rebuild CLI while GUI is running.

- `cargo test --workspace`; explicitly run recommendation 10k/100k ignored scale test; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo build -p wardrobe-cli`.
- `python3 docs/verification/ticket-16/reproduce-crash.py /absolute/target/debug/wardrobe-cli`.
- `python3 docs/verification/ticket-16/reproduce-entry.py /absolute/target/debug/wardrobe-cli` exercises limited CLI and stdin MCP against shared constraints. Token exists only in temporary process memory and is omitted from output.
- Start the verified local Qwen GGUF with the existing llama-server, localhost 18086, alias wardrobe-qwen, context8192, CPU2. `WARDROBE_ACTUAL_MODEL=wardrobe-qwen python3 docs/verification/ticket-16/reproduce-actual.py /absolute/target/debug/wardrobe-cli` captures public weather and raw actual model request/response. Stop the server afterwards.
- Frontend build and Playwright: `WARDROBE_ENCRYPTION_OS_TEST=1 WARDROBE_AI_ACTUAL_ENDPOINT=http://127.0.0.1:18086/v1 WARDROBE_TEST_PORT=5187 npm run test:e2e`. Real actual-model test is gated by the explicit endpoint; a skip is not a pass.

## Limits

This verifies a small local model's legal-ID selection, not general styling accuracy. Its recorded explanation can be weak or inaccurate and stays marked unverified. The free weather endpoint is an authorized noncommercial example, not a purchased commercial service. Native Tauri GUI, other OS execution, paid cloud models and full Beta/#35 platform gates are not claimed. Generic compatible chat has no outcome query; crash/unknown outcomes require provider inspection or an explicit new offline intent, never blind same-key paid retries.

## First candidate verification (6635969, before independent rejection)

These historical checks used first-candidate source `6635969` after merging root `9a5698a`; the repaired candidate results follow below.

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

At the first candidate handoff, local Qwen was retained at the parent's request, localhost18086, PID10390 / original tool handle22569. The independent merger subsequently terminated that server; this is historical evidence. Its weight SHA is in model-download.json; alias wardrobe-qwen, context8192, parallel1, CPU2. Independent merger owns the subsequent shutdown; do not run another server on that port. The implementation's Cargo/Playwright handles are terminal. No user key, actual wardrobe or paid cloud call was used. Root cache is released after candidate handoff.

Implementation review of the four issue criteria: actual weather/model demonstration, shared current-inventory constraints with bounded fallback, offline/manual three-entry availability, and reproducible public tests/ADR/glossary are demonstrated here; **independent merger acceptance remains pending**. General model explanation quality, native Tauri execution, other OS, paid providers, full sync of new weather configuration and full Beta/#35 gates are not claimed.


## Independent findings and asynchronous repair

The independent merger checkpoint `2a3898b` is preserved, including its two P2 findings and genuinely red 429→200 contract (`independent/merge16-rate-limit-red.log`, original handle47816). This implementation did not originate that red. The new local red `logs/async-immediate-red.log` observed409.93ms and weather HTTP before receipt; its final contract directly measures initial weather HTTP0/model POST0, reopened queued job identity and worker-only HTTP, independently of the elapsed threshold.

AI generate (and AI-derived replace/change) now returns a durable job receipt before HTTP. The caller may query jobs.get immediately and explicitly execute recommendation.worker.run through an authorized CLI or MCP client. GUI starts the worker, polls durable state and offers cancel; result.intent_id refers to the same immutable intent used by get/save. Saving still creates a template without wear. The completion helper in public tests explicitly performs enqueue→worker→get; it does not pretend the initial call returns an intent.

A confirmed429 records Retry-After (seconds or HTTP-date), global rate_retries and next_due under the current lease. At most two automatic rate retries are separate from the two invalid-output phases, allowing at most four POSTs when combined. Short waits check cancellation/expiry; delays beyond30 seconds leave queued next_due and release the worker. They are not silently shortened. Standalone callers run the explicit worker when due; the active GUI drives that queue. Unknown submitted outcomes still cannot be retried blindly.

`logs/async-durable-full-contract.log` verifies eleven public behaviors: genuine Retry-After1 second then success; independent output/rate budgets across reopened workers using actual HTTP429(31s) and invalid responses; a documented clock seam advances only next_due for long delay tests, never counters; cancellation/expiry during a genuine retry wait prevents the second POST. Initial receipt measured461µs, weather0/model0, with durable queued state after reopening. Later focused logs cover the final response-usage and unknown CAS changes. Provider request counts and available token usage are recorded from actual responses, without invented cost.


## Repaired candidate final verification

| Check | Actual terminal result |
| --- | --- |
| Workspace Rust | 171 passed, 0 failed, 12 ignored; `logs/async-workspace-final.log` |
| Full original scale | 10,000 garments / 100,000 effective wears, 5/5 results, fixed preserved, 4808 ms, DB159842304 bytes; `logs/async-scale-final.log` |
| Clippy / same-source CLI | Passed; `logs/async-clippy-final.log`, `logs/async-cli-build.log` |
| Actual OS temporary-key tests | 6 encryption + 3 encrypted packages passed separately; `logs/async-os-final.log` |
| Frontend / complete GUI | Build passed; 48 passed, 3 skipped; `logs/async-ui-build-final.log`, `logs/async-gui-final.log` |
| Final UI labels | Chinese durable task state labels were the only source change after the full GUI run; frontend rebuild and the three weather/actual-model/cancellation GUI cases passed, `logs/async-gui-label-final.log`. These are repeated checks, not additional distinct passes. |
| Actual async public weather + Qwen | `async-actual.json`: initial receipt10.01ms, modelPOST0/cache null before worker; actual Shanghai13.5°C with obtained/valid timestamps; seven synthetic garments, one fixed coat, five rule candidates, one real model request, legal four-item result, source text_model, wear0; actual usage2196 prompt +276 completion =2472 tokens |
| Limited CLI / stdin MCP | `async-entry.json`: durable enqueue/query/worker/get plus replace/change/save, one saved template, wear0; actual model network failure gives rules |
| Actual kill / restart | `async-crash.json`: real worker SIGKILL after one observed POST; restarted worker does not resubmit, documented lease-clock seam leads waiting_input; generic retry refused, explicit offline intent works, wear0 |

The unknown-outcome transition rereads state in the same IMMEDIATE transaction, preserving cancellation, known rejection and an already-published intent. Publication checks the lease before INSERT/commit; the unit contract proves post-publication cancellation/finish failure still leaves a same-key intent reference without another POST. Output phases and confirmed-rate retry counters survive worker claims/restarts. Local compile/Clippy fixture errors are retained in logs and are not counted as domain red.

Reproduce using `/tmp/wardrobe-implementation/ticket16-env.sh` and the same-source CLI at `/Users/links/.codex/worktrees/9729/电子衣橱 Agent/target/debug/wardrobe-cli`:

- Run `reproduce-crash-async.py` and `reproduce-entry-async.py`, passing that absolute CLI path.
- Run `WARDROBE_ACTUAL_MODEL=wardrobe-qwen python3 docs/verification/ticket-16/reproduce-actual-async.py /absolute/target/debug/wardrobe-cli` for the real weather/model workflow.
- Frontend full GUI uses `WARDROBE_ENCRYPTION_OS_TEST=1 WARDROBE_AI_ACTUAL_ENDPOINT=http://127.0.0.1:18086/v1 WARDROBE_TEST_PORT=5187 npm run test:e2e`. Actual-model tests are explicitly gated; skipped tests are not passes.

Owned Qwen server remains running for the parent's independent merger at its request: PID34439, tool handle39531, localhost18086, log `/tmp/wardrobe-implementation/ticket16-qwen-async-server.log`. Restart if needed: `/tmp/wardrobe-local-provider/build/bin/llama-server -m /tmp/wardrobe-local-provider/weights/qwen2.5-0.5b-instruct-q4_k_m.gguf -c 8192 --parallel 1 --host 127.0.0.1 --port 18086 --alias wardrobe-qwen -t 2 -tb 2 --n-gpu-layers 0`. Existing official model SHA/license evidence remains in `model-download.json`; no new download or key was used. Build/test/GUI handles are terminal and cache is released at handoff. `async-cache-cleanup.json` records23 terminal test executables,2511104224 bytes removed, preserving CLI/libraries/build/models/evidence.

All four issue criteria remain demonstrated at implementation level, including actual public weather and legal real-model selection, shared hard/fresh inventory constraints and bounded fallback, offline/manual GUI/CLI/MCP flows, and reproducible contracts/documentation. Independent acceptance is pending. Paid cloud, native Tauri window, other OS execution, styling accuracy, future sync and full Beta/#35 platform gates remain unclaimed.
