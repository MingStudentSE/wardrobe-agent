# Ticket 18 — Standards Review (standards axis only)

Verified: HEAD `7a4e3a21a2230ab9c6ddcb1cfee7172abd95f10b`, baseline `b6ee5c8…`; diff nonempty (127 files, +9546/−21; 20 non-evidence source files reviewed). Tests/build not run per constraints. Sources: AGENTS.md, ADR `docs/adr/0025-managed-preferences-and-event-learning.md`, GLOSSARY.md, `docs/04-UI设计与跨端适配.md`, docs/agents/*, pinned `.codex/skills/hallmark`.

## Hard findings (baseline-smell violations)

1. **Duplicated Code** — garment-snapshot feature extraction appears in two files with identical shape: `crates/wardrobe-core/src/recommendations.rs` (new `learning_scores` hunk: loops `["color","style","material","pattern","category","occasion"]`, collects `field.as_str()` + `field.as_array()` strings) duplicates `crates/wardrobe-core/src/learning.rs:460-475` (`absorb`). Extract a shared `snapshot_features()` helper.
2. **Duplicated Code** — the ADR0025 weight formula is coded twice in the same change: `learning.rs:478-488` (`absorb`: base + `overall-3`; material + `comfort-3`; color/pattern/style + `style-3`) vs `learning.rs:982-991` (`validate_portable_document`), including the reaction match `like=>2, dislike=>-2` at 505-509 vs 976-981. Divergence would make the recomputability validator (ADR0025 §学习领域校验) reject or accept wrongly. Extract one pure `signal_weight(base, scores, dimension)`.
3. **Duplicated Code** — tombstone block duplicated within `learning_manage`: `learning.rs:138-147` and `159-167` (clone current, new uuid, `revision+1`, `deleted=true`, INSERT into learning_memories).
4. **Primitive Obsession / Repeated Switches** — the six-dimension whitelist is re-declared as string literals 6×: learning.rs:122-125, 460-462, 711-714, 791 (descriptor enum), 813-814, recommendations.rs hunk; UI copy in `LearningMemory.tsx` `dimensions`. Repo precedent (`pub use clothing::CATEGORIES`, lib.rs:34) shows the expected named constant; `(String,String)` feature keys (`learning.rs:4`) are the same clump.

## Heuristic smells (judgment calls)

- Idempotency-receipt lookup repeated 3× (learning.rs:79-85, 392-413, 739-748) and `operation_key` validation 4× (51, 253, 377, 707) — small variants; a helper would consolidate.
- `package.rs` `learning_doc` `_ =>` arm whitelists a union key list for memories/history/receipts — fixed keys, so not ADR's forbidden "catchall导出", but per-table lists would be tighter.
- `backup.rs` `TABLES[..TABLES.len()-5]` couples the DB19/DB20 split to list position; a named `LEARNING_TABLES` const is safer.
- Mysterious names: `absorb` (learning.rs:456); `strip_learning_local_epoch` (backup.rs) removes `local_source_version`/`local_installation_generation`, no "epoch" field exists.

## UI / hallmark (docs/04 + AGENTS.md)

No violation: `LearningMemory.css` provides 48px targets (summary, `.learning-check`), focus-visible outline, 768px grid fallback; no hover-only, drag, or wide-window-critical flow; `title=` tooltip has inline-text equivalent (docs/04 line 39).

No speculative generality, middle-man, or divergent-change violations found; `parse`/`invalid` helpers and per-domain `validate_portable_document` follow feedback.rs/weather.rs conventions.
