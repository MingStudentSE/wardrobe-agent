Independent Spec re-review: repaired candidate 98fa8453aa31ee37c10a85095cf77448f53886a3, baseline 9a5698a; repair diff inspected against rejected 2a3898b. Read-only source review; author logs are not independent runtime proof.

Original P2 findings resolved by source inspection (2 resolved, 0 remaining actionable findings).

1. Parent #1 and docs/05 §3.3: “网络/限流最多自动重试 2 次并遵守 retry-after.” ai_recommendations.rs:417–459 now recognizes confirmed 429 responses, persists rate-limit outcome/deadline, waits or requeues for longer deadlines, and enforces the durable global retry budget in :484–520. :594–607 supports both delay seconds and an HTTP date. Invalid-output retry is separately bounded through output_attempt (:273–276 and :495–499), so reconstructing a worker does not reset either allowance. Unknown submissions remain non-resubmittable (:116–144).

2. Parent shared API: “长任务立即返回任务 ID.” recommendation.generate now calls ai_enqueue; ai_recommendations.rs:52–91 durably enqueues and returns a receipt with job_id without weather/provider HTTP. recommendation.worker.run is a shared Application operation (recommendations.rs:16–17 and :919–923), and GUI receives the ID before separately starting a worker and polling jobs.get, with cancellation visible (Recommendations.tsx:24–38). Persistent jobs remain selectable after reload (:20, :44).

The original strict ID/member/role/fixed/excluded/current-availability checks remain in the recommendation publication path. Publication still checks the current lease inside the same IMMEDIATE transaction; cancellation/expiry cannot adopt a late response. Explicit manual/off weather and persistent cache/configuration remain aligned with ticket #16 and F09/F10/F23. No requested ticket requirement was removed in the repair, and no unrelated product scope was added.

This conclusion is source-level only. Actual zero-HTTP enqueue, restart/deferred retry timing, concurrency/cancellation, CLI/MCP, real weather/model and GUI demonstrations must be certified by the parent merger’s independent runs. It does not close full Beta platform or paid-image-provider gates.
