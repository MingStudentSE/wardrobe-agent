Hallmark critique: P4 H4 E4 S4 R5 V4.

Standards re-review: fixed point 9a5698a, repair candidate 98fa8453aa31ee37c10a85095cf77448f53886a3. Independently inspected full source delta and repair delta from 2a3898b; re-read AGENTS.md, domain rules, design.md, ADR0023 and the shared API/job/credential decisions. Applied pinned Hallmark audit against the existing Atelier system. No builds or source edits; author logs are not acceptance evidence here.

Documented standards violations: 0. Repair adds immediate durable receipts and an explicit member-scoped Application worker, preserving the shared API and persisted job terminology. Submitted-state protection, invalid-output budget and separate 429 budget are recorded in jobs, checked before POST, and reflected in ADR0023. Model usage displays reported values without fabricated cost estimates. GUI exposes text-labelled status, cancellation, task selection and continuation through native click/keyboard controls; cancellation remains available while the main form is blocked. No new colours/fonts, hover-only controls or window-width-dependent critical actions were introduced. Full native/platform and actual viewport acceptance remain separate gates.

Judgement smells: 2, nonblocking.
1. Duplicated Code: recommendations.rs generate_fields and recommendation_build retain overlapping accepted-field lists and key/timezone/limit checks, with ai_enqueue repeating the latter. This creates several places to update when adding a control. Prefer one authoritative request validator, retaining separate schemas for distinct operations.
2. Data Clumps: ai_recommendations.rs text_submitted/text_usage/text_rate_limited/text_defer_rate_limit/text_outcome repeatedly travel with member/job/lease. A private claimed-task identity could reduce mismatches as helpers grow. This is a local refactoring opportunity, not a reason to add a general provider/task framework.

These heuristic observations do not establish present incorrect behavior. The earlier two P2 specification findings require their own independent specification/correctness verdict; this Standards pass does not supersede that axis.
