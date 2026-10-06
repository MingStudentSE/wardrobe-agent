# ADR 0023: Configured weather and validated text recommendations

Status: accepted implementation decision; acceptance requires independent verification.

Weather uses an explicit member location and coordinates. Off and manual temperature/occasion remain available. No location is inferred. The free Open-Meteo adapter is an explicit noncommercial option; the application does not purchase a plan. Estimates retain source, provider valid time, retrieval time, attribution, and error. A cached estimate becomes stale strictly after three hours. A failed refresh retains the previous estimate and permits offline rules. A configuration compare-and-swap prevents a late refresh from restoring a disabled or changed location.

The text model organizes candidate sets produced by the existing rule planner. It receives bounded candidate facts and controls, never original images, local paths, secrets, or an unbounded database dump. Candidate preparation does not persist another recommendation intent. The same planner validates body coverage, current member availability, fixed/excluded IDs, roles/layers, recent wear and cold-weather outerwear again after the network call. Replace/change preserve retained immutable metadata. Saving a recommendation creates a template, never a wear event.

Plain compatible chat is the default. JSON object and llama.cpp JSON schema modes require explicit selection; no unsupported structured parameter is silently imposed on generic providers. Two confirmed invalid outputs permit at most two submissions; other failures fall back to rules. Model explanations are unverified text and do not alter clothing facts.

A persisted `text_recommendation` job claims a lease before submission. The transaction records submission before POST. Two connections with the same operation key cannot submit twice. A second POST is permitted only after a confirmed invalid first response, with the same valid lease. A submitted request with unknown outcome cannot be blindly retried after a crash; generic chat has no outcome query, so the caller must check the provider or explicitly select offline rules/new intent. Submitted expired leases move to waiting_input. Cancellation/expiry prevents publication, checked in the same IMMEDIATE transaction that inserts the intent. If cancellation occurs after that commit but before job completion, the already published intent remains authoritative and same-key retry returns it without another network call.

Schema 18 adds only persisted weather preferences. Jobs reuse the existing table. Table presence is repaired even when user_version already reaches a newer module version. Weather cache is derived; future sync should export configuration explicitly rather than provider/cache/request credentials.

Primary references: [Open-Meteo forecast protocol](https://open-meteo.com/en/docs), [free-service terms](https://open-meteo.com/en/terms), [official llama.cpp server protocol](https://github.com/ggml-org/llama.cpp/blob/master/tools/server/README.md), [official Qwen GGUF model and license](https://huggingface.co/Qwen/Qwen2.5-0.5B-Instruct-GGUF).

Actual local model selection proves legal IDs for one public synthetic wardrobe, not general styling accuracy. Paid cloud providers, native platform GUI execution, and full Beta platform gates remain separate evidence requirements.
