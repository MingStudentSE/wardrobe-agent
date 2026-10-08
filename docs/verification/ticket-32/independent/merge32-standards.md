Hallmark critique: P4 H4 E4 S5 R5 V4.

Standards axis: fixed point 1bd7737, candidate 1451d47db771f3004feef76889fe1f9ca3e7ecd6. Independently inspected source delta and commit log, AGENTS/domain rules, GLOSSARY/ADR0022 with shared API/authorization/history decisions, design.md and UI rules. Pinned Hallmark audit applied to the added controls/CSS. No builds, source edits or cache writes; runtime acceptance belongs to the merger.

Documented standards violations: 0 found. Portable backups use explicit factual tables and a fresh white-list database, excluding installation grants, wallets, providers, jobs and submission guards; derived weather cache/error is cleared. Feedback observations and historical asset references survive without rewriting them as current inventory. Restore increments local generation, preserves unknown submitted-call guards and records maintenance completion inside the authoritative transaction. Authorized execution uses execute_current after grant validation, while preauthorization reopening carries the original token; global maintenance is denied for member-restricted grants and job controls additionally require the original operation scope. Directory relocation distinguishes same-installation transfer from portable restoration and retains the old source. GUI uses shared tokens/native labelled controls, text status/error, explicit destructive confirmation and accessible cancellation/query/continuation. No hover-only or width-dependent critical path was added.

The insights test delta retains all existing business assertions. Added enable/value checks await actual member acknowledgement; the new delayed-ack contract forwards the real request, then verifies old read-only and new-member contents rather than substituting responses.

Judgement smells: 2, nonblocking.
1. Divergent Change: backup.rs combines settings, archive production, preview validation and restore orchestration in a large operation dispatcher. Splitting archive validation/transport helpers from application orchestration would make later fact-module changes more local; the explicit fact allowlist should remain authoritative.
2. Duplicated Code: filesystem streaming/checksum/regular-file validation recurs across backup.rs and data_directory.rs. A small owned-file helper could prevent drift, while retaining distinct portable-package and same-installation policies. Avoid a speculative generic storage framework.

No claim of full native/platform or capacity acceptance follows from this standards inspection.
