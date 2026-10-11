use serde_json::{json, Value};
use wardrobe_core::Application;
// F17 first vertical seam (ticket-22): natural-language inventory query over a read-only
// tool loop. Public Application regression only:
//   stylist.ask {member_id,operation_key,provider_id,question,timezone} -> durable queued receipt
//     BEFORE any model HTTP request;
//   stylist.worker.run {member_id,id} -> leased job doc after exactly two real localhost
//     chat-completions POSTs (tool turn clothing.list {member_id,limit:24}, then a grounded
//     answer citing JSON pointers into the actual tool result);
//   result contract: facts:[{call_id,path,value}], tool_calls:[{call_id,operation,arguments,result}],
//     budget:{model_turns,tool_calls}; no WearEvents.
// The model fixture is the established controlled local HTTP oracle (scripted replies,
// bounded accept count, stop flag, joined worker) — no paid calls, no credentials.

/// One scripted fixture turn: either a model reply (replayed under the default OpenAI
/// `choices.message.content` envelope) or an accepted POST whose connection is closed
/// without any response bytes, deliberately leaving the submission outcome unknown.
enum ScriptedTurn {
    Reply(Value),
    CloseWithoutResponse,
    /// 带状态码的空响应体回复（如 429 限流）。
    Status(u16),
}

/// Bounded localhost chat-completions fixture: replays scripted JSON bodies in order,
/// captures raw request texts, accepts at most one connection per reply, and stops
/// cleanly via stop flag + Drop join (no dangling threads even when the test panics RED).
struct ScriptedModel {
    endpoint: String,
    posts: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    requests: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    worker: Option<std::thread::JoinHandle<()>>,
}
impl ScriptedModel {
    fn start(replies: Vec<Value>) -> Self {
        Self::start_turns(replies.into_iter().map(ScriptedTurn::Reply).collect())
    }
    /// Fixture extension for the security oracles: individual turns may answer normally
    /// or be accepted-then-closed; every accepted POST is counted and captured either way.
    fn start_turns(turns: Vec<ScriptedTurn>) -> Self {
        use std::io::Write;
        use std::sync::{
            atomic::{AtomicBool, AtomicUsize, Ordering},
            Arc, Mutex,
        };
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = format!("http://{}/v1", listener.local_addr().unwrap());
        let posts = Arc::new(AtomicUsize::new(0));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let (p, r, done) = (posts.clone(), requests.clone(), stop.clone());
        let worker = std::thread::spawn(move || {
            for turn in turns {
                let mut stream = loop {
                    if done.load(Ordering::SeqCst) {
                        return;
                    }
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::sleep(std::time::Duration::from_millis(2));
                        }
                        Err(_) => return,
                    }
                };
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                    .unwrap();
                let request = read_bounded_request(&mut stream);
                p.fetch_add(1, Ordering::SeqCst);
                r.lock().unwrap().push(request);
                match turn {
                    ScriptedTurn::Reply(reply) => {
                        let body = json!({"choices":[{"message":{"content":reply.to_string()}}]})
                            .to_string();
                        let _ = write!(
                            stream,
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            body.len(),
                            body
                        );
                    }
                    // Counted and captured, then closed with zero response bytes.
                    ScriptedTurn::CloseWithoutResponse => drop(stream),
                    ScriptedTurn::Status(code) => {
                        let reason = if code == 429 { "Too Many Requests" } else { "Error" };
                        let _ = write!(
                            stream,
                            "HTTP/1.1 {code} {reason}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                        );
                    }
                }
            }
        });
        Self {
            endpoint,
            posts,
            requests,
            stop,
            worker: Some(worker),
        }
    }
    fn post_count(&self) -> usize {
        self.posts.load(std::sync::atomic::Ordering::SeqCst)
    }
    fn request(&self, index: usize) -> String {
        self.requests
            .lock()
            .unwrap()
            .get(index)
            .cloned()
            .expect("captured model request")
    }
}
impl Drop for ScriptedModel {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::SeqCst);
        if let Some(worker) = self.worker.take() {
            worker.join().unwrap();
        }
    }
}
/// Reads one HTTP request (head + Content-Length body) with a hard 1 MiB cap; a stalled
/// client is cut off by the stream read timeout instead of hanging the fixture.
fn read_bounded_request(stream: &mut std::net::TcpStream) -> String {
    use std::io::Read;
    let mut buffer = Vec::new();
    let mut chunk = [0; 8192];
    let head_end = loop {
        let read = stream.read(&mut chunk).unwrap_or(0);
        if read == 0 {
            break buffer.len();
        }
        buffer.extend_from_slice(&chunk[..read]);
        if let Some(position) = buffer.windows(4).position(|w| w == b"\r\n\r\n") {
            break position + 4;
        }
        assert!(buffer.len() <= 1_048_576, "fixture request exceeded 1 MiB");
    };
    let head = String::from_utf8_lossy(&buffer[..head_end.min(buffer.len())]).to_string();
    let content_length = head
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            if name.eq_ignore_ascii_case("content-length") {
                value.trim().parse::<usize>().ok()
            } else {
                None
            }
        })
        .unwrap_or(0);
    while buffer.len() < head_end + content_length {
        let read = stream.read(&mut chunk).unwrap_or(0);
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
        assert!(buffer.len() <= 1_048_576, "fixture request exceeded 1 MiB");
    }
    String::from_utf8_lossy(&buffer).to_string()
}

#[test]
fn first_read_only_tool_loop_returns_durable_receipt_then_server_grounded_answer() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("db");
    let app = Application::open(&path).unwrap();
    let space = app.execute("space.default", json!({})).unwrap();
    let member = space["member_id"].as_str().unwrap().to_string();
    // Two synthetic items through the real public operation; the loop may only ever
    // ground its answer in IDs the server actually returned for this member.
    let created: Vec<Value> = [("羊毛毛衣", "top"), ("直筒牛仔裤", "bottom")]
        .into_iter()
        .map(|(name, category)| {
            app.execute(
                "clothing.create",
                json!({"member_id":member,"name":name,"category":category}),
            )
            .unwrap()
        })
        .collect();
    let expected_ids: Vec<String> = created
        .iter()
        .map(|item| item["id"].as_str().unwrap().to_owned())
        .collect();
    // Real provider.save bound to the local fixture; text capability, no authentication.
    let model = ScriptedModel::start(vec![
        // Turn 1: model requests the whitelisted read-only operation, member-scoped, page 24.
        json!({"kind":"tool","operation":"clothing.list","arguments":{"member_id":member,"limit":24}}),
        // Turn 2: model answers by citing pointers into the actual tool result only.
        json!({"kind":"answer","facts":[{"call_id":0,"path":"/items/0/id"},{"call_id":0,"path":"/items/1/id"}],"explanation":"以上是当前可用衣物"}),
    ]);
    let provider = app
        .execute(
            "provider.save",
            json!({"name":"fixture stylist model","endpoint":model.endpoint,"model":"fixture-text","capabilities":["text"],"authentication":"none"}),
        )
        .unwrap();
    // RED(ticket-22): stylist.ask must return a durable queued receipt BEFORE any HTTP request.
    let receipt = app
        .execute(
            "stylist.ask",
            json!({"member_id":member,"operation_key":"first-read-only-loop","provider_id":provider["id"],"question":"今天穿什么","timezone":"UTC"}),
        )
        .expect(
            "RED(ticket-22): stylist.ask {member_id,operation_key,provider_id,question,timezone} is not implemented yet; \
             this is the anticipated genuine failure (current dispatch returns unsupported_operation). \
             The fixture and compiler are not the failure point.",
        );
    assert_eq!(receipt["state"], "queued", "receipt must be queued");
    let id = receipt["job_id"]
        .as_str()
        .expect("receipt carries the durable job id")
        .to_owned();
    assert_eq!(
        model.post_count(),
        0,
        "queued receipt must precede any model HTTP request"
    );
    // Durability: the queued job survives reopen with zero model traffic so far.
    let reopened = Application::open(&path).unwrap();
    let durable = reopened
        .execute("jobs.get", json!({"member_id":member,"id":id}))
        .unwrap();
    assert_eq!(durable["kind"], "stylist_query");
    assert_eq!(durable["state"], "queued");
    assert_eq!(model.post_count(), 0);
    // Worker: exactly two real localhost POSTs, then the leased job doc is finished.
    let job = reopened
        .execute("stylist.worker.run", json!({"member_id":member,"id":id}))
        .expect("stylist.worker.run must drive the first read-only tool loop");
    assert_eq!(job["state"], "succeeded");
    assert_eq!(
        model.post_count(),
        2,
        "exactly two model turns: tool request, then grounded answer"
    );
    assert!(
        model.request(0).contains("/chat/completions"),
        "first turn is a chat-completions POST"
    );
    assert!(
        model.request(0).contains("今天穿什么"),
        "the user question must reach the model"
    );
    assert!(
        model.request(0).contains("fixture-text"),
        "the saved provider's model must be used"
    );
    assert!(
        model.request(1).contains("/chat/completions")
            && model.request(1).contains(&expected_ids[0]),
        "second turn must carry the actual clothing.list result IDs for grounding"
    );
    // Exact root result contract: server-extracted facts, one actual tool result, visible budget.
    let result = &job["result"];
    let facts = result["facts"].as_array().expect("facts array");
    assert_eq!(facts.len(), 2);
    assert_eq!(facts[0]["call_id"], 0);
    assert_eq!(facts[0]["path"], "/items/0/id");
    assert_eq!(
        facts[0]["value"], expected_ids[0],
        "fact values are extracted by the server from the real tool result, never trusted from the model"
    );
    assert_eq!(facts[1]["call_id"], 0);
    assert_eq!(facts[1]["path"], "/items/1/id");
    assert_eq!(facts[1]["value"], expected_ids[1]);
    let tool_calls = result["tool_calls"].as_array().expect("tool_calls array");
    assert_eq!(tool_calls.len(), 1);
    assert_eq!(tool_calls[0]["call_id"], 0);
    assert_eq!(tool_calls[0]["operation"], "clothing.list");
    assert_eq!(
        tool_calls[0]["arguments"],
        json!({"member_id":member,"limit":24})
    );
    let tool_result = &tool_calls[0]["result"];
    assert_eq!(tool_result["schema_version"], 1);
    let items = tool_result["items"].as_array().expect("tool result items");
    assert_eq!(items.len(), 2);
    for created_item in &created {
        assert!(items.iter().any(|item| {
            item["id"] == created_item["id"]
                && item["name"] == created_item["name"]
                && item["category"] == created_item["category"]
        }));
    }
    assert!(
        tool_result["next_cursor"].is_null(),
        "pagination projection is retained; a capped page never proves a global total"
    );
    assert_eq!(
        result["budget"],
        json!({"model_turns":2,"tool_calls":1}),
        "budget must be visible: two model turns, one tool call"
    );
    // Read-only loop: no wear events may be created as a side effect.
    assert_eq!(
        reopened
            .execute("wear.list", json!({"member_id":member}))
            .unwrap()["items"],
        json!([]),
        "the first read-only tool loop must not record any WearEvents"
    );
    // Submission-aware: a finished job replays its stored result without new model POSTs.
    let replay = reopened
        .execute("stylist.worker.run", json!({"member_id":member,"id":id}))
        .unwrap();
    assert_eq!(replay["state"], "succeeded");
    assert_eq!(replay["result"], job["result"]);
    assert_eq!(
        model.post_count(),
        2,
        "no duplicate model POST after the job finished"
    );
}

// ---------------------------------------------------------------------------
// Ticket-22 security oracles: independent public-API regressions for the
// stored StylistOrigin, deferred grant revalidation, cross-client isolation,
// member-scoped tool dispatch, unknown submission outcomes, and the projected
// fact surface. Public Application/execute_as_client only; tokens are one-time
// generated ephemeral values and are never printed.
// ---------------------------------------------------------------------------

/// Opens an isolated per-test database inside the temp directory.
fn open_isolated_app(directory: &std::path::Path) -> Application {
    Application::open(&directory.join("db")).unwrap()
}

/// Bootstraps the real workspace and returns its actual default member ID.
fn bootstrap_member(app: &Application) -> String {
    app.execute("space.default", json!({})).unwrap()["member_id"]
        .as_str()
        .expect("workspace member id")
        .to_owned()
}

/// Saves a real text provider bound to the local fixture endpoint; no credentials.
fn save_fixture_text_provider(app: &Application, endpoint: &str) -> String {
    app.execute(
        "provider.save",
        json!({"name":"fixture stylist model","endpoint":endpoint,"model":"fixture-text","capabilities":["text"],"authentication":"none"}),
    )
    .unwrap()["id"]
        .as_str()
        .expect("provider id")
        .to_owned()
}

/// Registers a limited client through the trusted public seam and returns its
/// one-time ephemeral token plus grant id. The token is never logged or printed.
fn authorize_limited_client(
    app: &Application,
    member: &str,
    operations: &[&str],
) -> (String, String) {
    let issued = app
        .execute(
            "client.authorize",
            json!({"name":"security regression client","mode":"limited","operations":operations,"member_ids":[member],"expires_at":null}),
        )
        .expect("trusted client.authorize must issue a limited grant");
    let token = issued["token"].as_str().expect("one-time token").to_owned();
    assert_eq!(
        token.len(),
        64,
        "ephemeral client tokens are 64 hex characters"
    );
    let id = issued["id"].as_str().expect("grant id").to_owned();
    (token, id)
}

fn assert_member_has_no_wear(app: &Application, member: &str) {
    assert_eq!(
        app.execute("wear.list", json!({"member_id":member}))
            .unwrap()["items"],
        json!([]),
        "the read-only stylist loop must not record WearEvents for {member}"
    );
}

#[test]
fn client_without_clothing_list_scope_fails_scope_denied_after_a_single_post() {
    let d = tempfile::tempdir().unwrap();
    let app = open_isolated_app(d.path());
    let member = bootstrap_member(&app);
    app.execute(
        "clothing.create",
        json!({"member_id":member,"name":"羊毛毛衣","category":"top"}),
    )
    .unwrap();
    // The second reply exists only so a wrongly-allowed second turn fails fast RED
    // instead of stalling the fixture; the denial must end the loop after one POST.
    let model = ScriptedModel::start(vec![
        json!({"kind":"tool","operation":"clothing.list","arguments":{"member_id":member,"limit":24}}),
        json!({"kind":"answer","facts":[],"explanation":"工具被拒后不应到达第二轮"}),
    ]);
    let provider = save_fixture_text_provider(&app, &model.endpoint);
    let (token, _grant_id) = authorize_limited_client(
        &app,
        &member,
        &["stylist.ask", "stylist.worker.run", "jobs.get"],
    );
    let receipt = app
        .execute_as_client(
            &token,
            "stylist.ask",
            json!({"member_id":member,"operation_key":"scope-denied-loop","provider_id":provider,"question":"今天穿什么","timezone":"UTC"}),
        )
        .expect("stylist.ask is within the client scope");
    assert_eq!(receipt["state"], "queued");
    let id = receipt["job_id"].as_str().unwrap().to_owned();
    let job = app
        .execute_as_client(
            &token,
            "stylist.worker.run",
            json!({"member_id":member,"id":id}),
        )
        .unwrap();
    assert_eq!(job["state"], "failed");
    assert_eq!(
        job["error"]["code"], "scope_denied",
        "the deferred tool dispatch must re-apply the client grant scope"
    );
    assert!(
        job["result"].is_null(),
        "a denied tool dispatch must not publish any result"
    );
    assert_eq!(
        model.post_count(),
        1,
        "exactly one POST: the tool request; no second turn may follow the denial"
    );
    assert!(
        model.request(0).contains("clothing.list"),
        "the single POST is the model's clothing.list tool turn"
    );
    // The durable failure is visible through the client's permitted jobs.get.
    let stored = app
        .execute_as_client(&token, "jobs.get", json!({"member_id":member,"id":id}))
        .unwrap();
    assert_eq!(stored["state"], "failed");
    assert_eq!(stored["error"]["code"], "scope_denied");
    assert_eq!(
        stored["stylist"]["tool_calls"],
        json!([]),
        "the denied dispatch must not record any tool result"
    );
    assert_member_has_no_wear(&app, &member);
}

#[test]
fn revoked_client_origin_fails_trusted_worker_before_any_post() {
    let d = tempfile::tempdir().unwrap();
    let app = open_isolated_app(d.path());
    let member = bootstrap_member(&app);
    // Zero scripted replies: a correct run never reaches the model at all.
    let model = ScriptedModel::start(vec![]);
    let provider = save_fixture_text_provider(&app, &model.endpoint);
    let (token, grant_id) =
        authorize_limited_client(&app, &member, &["stylist.ask", "stylist.worker.run"]);
    let receipt = app
        .execute_as_client(
            &token,
            "stylist.ask",
            json!({"member_id":member,"operation_key":"revoke-before-run","provider_id":provider,"question":"今天穿什么","timezone":"UTC"}),
        )
        .unwrap();
    assert_eq!(receipt["state"], "queued");
    let id = receipt["job_id"].as_str().unwrap().to_owned();
    app.execute("client.revoke", json!({"id":grant_id}))
        .unwrap();
    // The trusted worker must honor the stored Client origin: the deferred grant is
    // revalidated afresh, so a revoked enqueue can never ride a trusted dispatch.
    let job = app
        .execute("stylist.worker.run", json!({"member_id":member,"id":id}))
        .unwrap();
    assert_eq!(job["state"], "failed");
    assert_eq!(job["error"]["code"], "authorization_revoked");
    assert!(
        job["result"].is_null(),
        "a revoked enqueue must not publish any result"
    );
    assert_eq!(
        model.post_count(),
        0,
        "revocation must prevent any model POST, including through the trusted worker"
    );
    assert_member_has_no_wear(&app, &member);
}

#[test]
fn second_client_cannot_run_cancel_or_retry_first_clients_stylist_job() {
    let d = tempfile::tempdir().unwrap();
    let app = open_isolated_app(d.path());
    let member = bootstrap_member(&app);
    let model = ScriptedModel::start(vec![]);
    let provider = save_fixture_text_provider(&app, &model.endpoint);
    let operations = [
        "stylist.ask",
        "stylist.worker.run",
        "jobs.get",
        "jobs.cancel",
        "jobs.retry",
    ];
    let (token_a, _grant_a) = authorize_limited_client(&app, &member, &operations);
    let (token_b, _grant_b) = authorize_limited_client(&app, &member, &operations);
    let receipt = app
        .execute_as_client(
            &token_a,
            "stylist.ask",
            json!({"member_id":member,"operation_key":"client-a-job","provider_id":provider,"question":"今天穿什么","timezone":"UTC"}),
        )
        .unwrap();
    let id = receipt["job_id"].as_str().unwrap().to_owned();
    // Same member, same operations, different grant: the stored origin still binds
    // the queued job to client A alone, for every management surface.
    for operation in ["stylist.worker.run", "jobs.cancel", "jobs.retry"] {
        let error = app
            .execute_as_client(&token_b, operation, json!({"member_id":member,"id":id}))
            .err()
            .unwrap_or_else(|| panic!("client B must not manage client A's job via {operation}"));
        assert_eq!(
            error.code, "scope_denied",
            "client B attempt via {operation} must be scope_denied"
        );
    }
    assert_eq!(
        model.post_count(),
        0,
        "no model traffic may be triggered by another client's management attempt"
    );
    let stored = app
        .execute("jobs.get", json!({"member_id":member,"id":id}))
        .unwrap();
    assert_eq!(
        stored["state"], "queued",
        "client B attempts must leave client A's job untouched and queued"
    );
}

#[test]
fn tool_request_for_foreign_member_fails_member_denied_after_a_single_post() {
    let d = tempfile::tempdir().unwrap();
    let app = open_isolated_app(d.path());
    let member = bootstrap_member(&app);
    let foreign = app
        .execute("member.create", json!({"name":"另一成员"}))
        .unwrap()["member_id"]
        .as_str()
        .unwrap()
        .to_owned();
    app.execute(
        "clothing.create",
        json!({"member_id":foreign,"name":"他人外套","category":"outerwear"}),
    )
    .unwrap();
    // Second reply exists so a wrongly-allowed second turn fails fast RED.
    let model = ScriptedModel::start(vec![
        json!({"kind":"tool","operation":"clothing.list","arguments":{"member_id":foreign,"limit":24}}),
        json!({"kind":"answer","facts":[],"explanation":"越权后不应到达第二轮"}),
    ]);
    let provider = save_fixture_text_provider(&app, &model.endpoint);
    let receipt = app
        .execute(
            "stylist.ask",
            json!({"member_id":member,"operation_key":"foreign-member-loop","provider_id":provider,"question":"今天穿什么","timezone":"UTC"}),
        )
        .unwrap();
    let id = receipt["job_id"].as_str().unwrap().to_owned();
    let job = app
        .execute("stylist.worker.run", json!({"member_id":member,"id":id}))
        .unwrap();
    assert_eq!(job["state"], "failed");
    assert_eq!(
        job["error"]["code"], "member_denied",
        "the tool must never read another actual member, even one that exists"
    );
    assert!(
        job["result"].is_null(),
        "a member-denied tool request must not publish any result"
    );
    assert_eq!(
        model.post_count(),
        1,
        "the foreign-member tool frame is answered by exactly one POST before denial"
    );
    assert_eq!(
        job["stylist"]["tool_calls"],
        json!([]),
        "no foreign tool result may enter the durable ledger"
    );
    assert_member_has_no_wear(&app, &member);
    assert_member_has_no_wear(&app, &foreign);
}

#[test]
fn provider_disconnect_parks_waiting_input_and_blocks_blind_resubmission() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("db");
    let app = Application::open(&path).unwrap();
    let member = bootstrap_member(&app);
    // The fixture accepts the single POST, then closes without any response bytes.
    let model = ScriptedModel::start_turns(vec![ScriptedTurn::CloseWithoutResponse]);
    let provider = save_fixture_text_provider(&app, &model.endpoint);
    let receipt = app
        .execute(
            "stylist.ask",
            json!({"member_id":member,"operation_key":"disconnect-park","provider_id":provider,"question":"今天穿什么","timezone":"UTC"}),
        )
        .unwrap();
    let id = receipt["job_id"].as_str().unwrap().to_owned();
    let job = app
        .execute("stylist.worker.run", json!({"member_id":member,"id":id}))
        .unwrap();
    assert_eq!(
        job["state"], "waiting_input",
        "an accepted POST with an unknown outcome parks the job for reconciliation"
    );
    assert!(
        matches!(
            job["error"]["code"].as_str(),
            Some("provider_network" | "provider_response")
        ),
        "the parked job records the unknown transport outcome"
    );
    assert_eq!(model.post_count(), 1);
    // Durability: after reopen the parked job replays without any extra POST.
    let reopened = Application::open(&path).unwrap();
    let parked = reopened
        .execute("stylist.worker.run", json!({"member_id":member,"id":id}))
        .unwrap();
    assert_eq!(parked["state"], "waiting_input");
    assert_eq!(
        model.post_count(),
        1,
        "the reopened worker must not blind-resubmit an outcome-unknown job"
    );
    // The submission ledger blocks an explicit retry from re-spending quota.
    let error = reopened
        .execute("jobs.retry", json!({"member_id":member,"id":id}))
        .expect_err("jobs.retry must refuse a job whose submission outcome is unknown");
    assert_eq!(error.code, "submission_already_sent");
    assert_eq!(
        reopened
            .execute("jobs.get", json!({"member_id":member,"id":id}))
            .unwrap()["state"],
        "waiting_input",
        "the refused retry leaves the parked job unchanged"
    );
    assert_member_has_no_wear(&reopened, &member);
}

#[test]
fn tool_loop_covers_todays_outfit_and_analysis_with_real_deterministic_queries() {
    let d = tempfile::tempdir().unwrap();
    let app = open_isolated_app(d.path());
    let member = bootstrap_member(&app);
    let created: Vec<Value> = [("羊毛毛衣", "top"), ("直筒牛仔裤", "bottom")]
        .into_iter()
        .map(|(name, category)| {
            app.execute(
                "clothing.create",
                json!({"member_id":member,"name":name,"category":category}),
            )
            .unwrap()
        })
        .collect();
    // The model tries to smuggle a second AI provider call into the tool frame;
    // the server must own the arguments and keep generation on local rules.
    let model = ScriptedModel::start(vec![
        json!({"kind":"tool","operation":"recommendation.generate","arguments":{"member_id":member,"occasion":"daily","limit":3,"strategy":"ai","provider_id":"smuggled-provider"}}),
        json!({"kind":"tool","operation":"insights.summary","arguments":{"member_id":member}}),
        json!({"kind":"answer","facts":[
            {"call_id":0,"path":"/proposals/0/items/0/clothing_id"},
            {"call_id":1,"path":"/inventory/total"},
            {"call_id":1,"path":"/never_worn_total"}
        ],"explanation":"第一套可用于今天；库存与统计来自查询"}),
    ]);
    let provider = save_fixture_text_provider(&app, &model.endpoint);
    let receipt = app
        .execute(
            "stylist.ask",
            json!({"member_id":member,"operation_key":"today-analysis-loop","provider_id":provider,"question":"今天穿什么，并分析我的衣橱","timezone":"UTC"}),
        )
        .unwrap();
    let id = receipt["job_id"].as_str().unwrap().to_owned();
    let job = app
        .execute("stylist.worker.run", json!({"member_id":member,"id":id}))
        .unwrap();
    assert_eq!(job["state"], "succeeded", "{job}");
    assert_eq!(model.post_count(), 3, "two tool turns then the answer");
    // The tool ledger carries only server-owned arguments: the smuggled AI
    // strategy never reaches the domain dispatch.
    let tool_calls = job["result"]["tool_calls"].as_array().unwrap();
    assert_eq!(tool_calls.len(), 2);
    let generate_arguments = &tool_calls[0]["arguments"];
    assert!(
        generate_arguments["strategy"].is_null(),
        "{generate_arguments}"
    );
    assert!(
        generate_arguments["provider_id"].is_null(),
        "{generate_arguments}"
    );
    assert_eq!(generate_arguments["member_id"], json!(member));
    assert_eq!(generate_arguments["occasion"], json!("daily"));
    assert_eq!(generate_arguments["limit"], json!(3));
    let generate_result = &tool_calls[0]["result"];
    assert_eq!(generate_result["schema_version"], 1);
    assert_eq!(generate_result["offline"], json!(true), "local rules only");
    assert_eq!(generate_result["creates_wear"], json!(false));
    let proposal_items = generate_result["proposals"][0]["items"]
        .as_array()
        .expect("at least one grounded proposal for a top+bottom inventory");
    assert!(!proposal_items.is_empty());
    for item in proposal_items {
        assert!(
            created.iter().any(|c| c["id"] == item["clothing_id"]),
            "proposal items must cite real inventory IDs: {item}"
        );
    }
    // Grounding: the follow-up prompt must carry the actual proposal items.
    assert!(
        model
            .request(1)
            .contains(proposal_items[0]["clothing_id"].as_str().unwrap()),
        "the second model turn must carry the real generate result"
    );
    // Deterministic analysis numbers come from the server-computed projection.
    let insights_result = &tool_calls[1]["result"];
    assert_eq!(insights_result["inventory"]["total"], json!(2));
    assert_eq!(insights_result["stale_total"], json!(0));
    assert_eq!(insights_result["never_worn_total"], json!(2));
    // The prompt JSON is embedded as an escaped string inside the HTTP body,
    // so the projection appears there in its escaped form.
    assert!(
        model.request(2).contains("\\\"total\\\":2"),
        "the answer turn must carry the real insights projection"
    );
    // Server-extracted facts resolve against the projected results only.
    let facts = job["result"]["facts"].as_array().unwrap();
    assert_eq!(facts.len(), 3);
    assert_eq!(
        facts[0]["value"], proposal_items[0]["clothing_id"],
        "fact values are extracted by the server, never trusted from the model"
    );
    assert_eq!(facts[1]["value"], json!(2));
    assert_eq!(facts[2]["value"], json!(2));
    assert_eq!(
        job["result"]["budget"],
        json!({"model_turns":3,"tool_calls":2})
    );
    // The rules generation persisted exactly one recommendation intent as a
    // domain artifact, but no WearEvents ever.
    let intents = app
        .execute("recommendation.list", json!({"member_id":member}))
        .unwrap();
    assert_eq!(intents["items"].as_array().unwrap().len(), 1);
    assert_eq!(intents["items"][0]["creates_wear"], json!(false));
    assert_member_has_no_wear(&app, &member);
    // A finished job replays without any further model traffic.
    let replay = app
        .execute("stylist.worker.run", json!({"member_id":member,"id":id}))
        .unwrap();
    assert_eq!(replay["result"], job["result"]);
    assert_eq!(model.post_count(), 3);
}

#[test]
fn tool_operation_outside_readonly_whitelist_fails_without_executing() {
    let d = tempfile::tempdir().unwrap();
    let app = open_isolated_app(d.path());
    let member = bootstrap_member(&app);
    let item = app
        .execute(
            "clothing.create",
            json!({"member_id":member,"name":"羊毛毛衣","category":"top"}),
        )
        .unwrap();
    // The model asks for a write tool with the REAL item id; the server must
    // refuse before any domain dispatch and the item must survive untouched.
    let model = ScriptedModel::start(vec![
        json!({"kind":"tool","operation":"clothing.delete","arguments":{"member_id":member,"id":item["id"]}}),
        json!({"kind":"answer","facts":[],"explanation":"写工具被拒后不应到达第二轮"}),
    ]);
    let provider = save_fixture_text_provider(&app, &model.endpoint);
    let receipt = app
        .execute(
            "stylist.ask",
            json!({"member_id":member,"operation_key":"write-tool-refused","provider_id":provider,"question":"帮我删掉这件衣服","timezone":"UTC"}),
        )
        .unwrap();
    let id = receipt["job_id"].as_str().unwrap().to_owned();
    let job = app
        .execute("stylist.worker.run", json!({"member_id":member,"id":id}))
        .unwrap();
    assert_eq!(job["state"], "failed");
    assert_eq!(job["error"]["code"], "tool_not_allowed");
    assert_eq!(model.post_count(), 1, "denial ends the loop after one POST");
    assert_eq!(
        job["stylist"]["tool_calls"],
        json!([]),
        "a refused tool must not enter the durable ledger"
    );
    let surviving = app
        .execute("clothing.get", json!({"member_id":member,"id":item["id"]}))
        .unwrap();
    assert_eq!(surviving["id"], item["id"], "the item must not be deleted");
    assert_member_has_no_wear(&app, &member);
}

#[test]
fn client_without_insights_scope_fails_deferred_tool_dispatch_scope_denied() {
    let d = tempfile::tempdir().unwrap();
    let app = open_isolated_app(d.path());
    let member = bootstrap_member(&app);
    // Second reply exists so a wrongly-allowed second turn fails fast RED.
    let model = ScriptedModel::start(vec![
        json!({"kind":"tool","operation":"insights.summary","arguments":{"member_id":member}}),
        json!({"kind":"answer","facts":[],"explanation":"工具被拒后不应到达第二轮"}),
    ]);
    let provider = save_fixture_text_provider(&app, &model.endpoint);
    let (token, _grant_id) =
        authorize_limited_client(&app, &member, &["stylist.ask", "stylist.worker.run"]);
    let receipt = app
        .execute_as_client(
            &token,
            "stylist.ask",
            json!({"member_id":member,"operation_key":"insights-scope-denied","provider_id":provider,"question":"分析我的衣橱","timezone":"UTC"}),
        )
        .unwrap();
    let id = receipt["job_id"].as_str().unwrap().to_owned();
    let job = app
        .execute_as_client(
            &token,
            "stylist.worker.run",
            json!({"member_id":member,"id":id}),
        )
        .unwrap();
    assert_eq!(job["state"], "failed");
    assert_eq!(
        job["error"]["code"], "scope_denied",
        "the insights tool is deferred under the same client grant revalidation"
    );
    assert_eq!(model.post_count(), 1);
    assert_eq!(job["stylist"]["tool_calls"], json!([]));
    assert_member_has_no_wear(&app, &member);
}

#[test]
fn explicit_wear_record_writes_once_replays_identical_retry_and_strips_forged_fields() {
    let d = tempfile::tempdir().unwrap();
    let app = open_isolated_app(d.path());
    let member = bootstrap_member(&app);
    let item = app
        .execute(
            "clothing.create",
            json!({"member_id":member,"name":"羊毛毛衣","category":"top"}),
        )
        .unwrap();
    let item_id = item["id"].as_str().unwrap().to_owned();
    // The user explicitly says 记一下今天穿了X; the model maps it to a write
    // tool frame. Forged operation_key/local_date must be stripped: the server
    // owns the idempotency key, member and calendar date. The identical repeat
    // frame (a model retry) must replay, never double-count the wear.
    let write_frame = || json!({"kind":"tool","operation":"wear.record","arguments":{"member_id":member,"clothing_ids":[item_id],"notes":"今天穿了毛衣","operation_key":"forged-key","local_date":"1999-01-01"}});
    let model = ScriptedModel::start(vec![
        write_frame(),
        write_frame(),
        json!({"kind":"answer","facts":[{"call_id":0,"path":"/id"}],"explanation":"已记录今天的穿着"}),
    ]);
    let provider = save_fixture_text_provider(&app, &model.endpoint);
    let receipt = app
        .execute(
            "stylist.ask",
            json!({"member_id":member,"operation_key":"write-wear-once","provider_id":provider,"question":"记一下今天穿了羊毛毛衣","timezone":"UTC"}),
        )
        .unwrap();
    let id = receipt["job_id"].as_str().unwrap().to_owned();
    let job = app
        .execute("stylist.worker.run", json!({"member_id":member,"id":id}))
        .unwrap();
    assert_eq!(job["state"], "succeeded", "{job}");
    assert_eq!(model.post_count(), 3, "write, identical retry, then answer");
    // The identical retry never became a second tool call: one write, one ledger entry.
    let tool_calls = job["result"]["tool_calls"].as_array().unwrap();
    assert_eq!(
        tool_calls.len(),
        1,
        "{}",
        serde_json::to_string(tool_calls).unwrap()
    );
    assert_eq!(tool_calls[0]["operation"], "wear.record");
    let arguments = &tool_calls[0]["arguments"];
    assert_eq!(arguments["clothing_ids"], json!([item_id]));
    assert_eq!(arguments["notes"], json!("今天穿了毛衣"));
    let key = arguments["operation_key"].as_str().unwrap();
    assert!(
        key.starts_with("stylist-") && key.contains(&id),
        "the server must own the idempotency key, not the model: {arguments}"
    );
    let today = chrono::Utc::now().date_naive().to_string();
    assert_eq!(arguments["local_date"], json!(today));
    assert_eq!(arguments["timezone"], json!("UTC"));
    // Server-extracted fact cites the real durable wear event id.
    let stored = app
        .execute("wear.list", json!({"member_id":member}))
        .unwrap();
    let events = stored["items"].as_array().unwrap();
    assert_eq!(
        events.len(),
        1,
        "the identical retry must not double-count the wear: {stored}"
    );
    assert_eq!(events[0]["kind"], json!("record"));
    assert_eq!(facts_of(&job)[0]["value"], events[0]["id"]);
    assert_eq!(
        job["result"]["budget"],
        json!({"model_turns":3,"tool_calls":1})
    );
    // The finished job replays without new writes or POSTs.
    let replay = app
        .execute("stylist.worker.run", json!({"member_id":member,"id":id}))
        .unwrap();
    assert_eq!(replay["result"], job["result"]);
    assert_eq!(model.post_count(), 3);
}

#[test]
fn write_tool_for_foreign_member_denied_without_any_side_effect() {
    let d = tempfile::tempdir().unwrap();
    let app = open_isolated_app(d.path());
    let member = bootstrap_member(&app);
    let foreign = app
        .execute("member.create", json!({"name":"另一成员"}))
        .unwrap()["member_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let foreign_item = app
        .execute(
            "clothing.create",
            json!({"member_id":foreign,"name":"他人外套","category":"outerwear"}),
        )
        .unwrap();
    // The model tries to edit another member's real item; the server must
    // refuse before any domain dispatch and the foreign item must survive.
    let model = ScriptedModel::start(vec![
        json!({"kind":"tool","operation":"clothing.update","arguments":{"member_id":foreign,"id":foreign_item["id"],"name":"篡改后的名字"}}),
        json!({"kind":"answer","facts":[],"explanation":"越权写不应到达第二轮"}),
    ]);
    let provider = save_fixture_text_provider(&app, &model.endpoint);
    let receipt = app
        .execute(
            "stylist.ask",
            json!({"member_id":member,"operation_key":"write-foreign-member","provider_id":provider,"question":"把那件外套改名","timezone":"UTC"}),
        )
        .unwrap();
    let id = receipt["job_id"].as_str().unwrap().to_owned();
    let job = app
        .execute("stylist.worker.run", json!({"member_id":member,"id":id}))
        .unwrap();
    assert_eq!(job["state"], "failed");
    assert_eq!(job["error"]["code"], "member_denied");
    assert_eq!(model.post_count(), 1, "denial ends the loop after one POST");
    assert_eq!(job["stylist"]["tool_calls"], json!([]));
    let surviving = app
        .execute(
            "clothing.get",
            json!({"member_id":foreign,"id":foreign_item["id"]}),
        )
        .unwrap();
    assert_eq!(
        surviving["name"], foreign_item["name"],
        "a denied write must leave the foreign item untouched"
    );
    assert_member_has_no_wear(&app, &member);
    assert_member_has_no_wear(&app, &foreign);
}

#[test]
fn client_without_wear_record_scope_fails_write_scope_denied_without_side_effect() {
    let d = tempfile::tempdir().unwrap();
    let app = open_isolated_app(d.path());
    let member = bootstrap_member(&app);
    let item = app
        .execute(
            "clothing.create",
            json!({"member_id":member,"name":"羊毛毛衣","category":"top"}),
        )
        .unwrap();
    // Second reply exists so a wrongly-allowed second turn fails fast RED.
    let model = ScriptedModel::start(vec![
        json!({"kind":"tool","operation":"wear.record","arguments":{"member_id":member,"clothing_ids":[item["id"]],"notes":"今天穿了"}}),
        json!({"kind":"answer","facts":[],"explanation":"写被拒后不应到达第二轮"}),
    ]);
    let provider = save_fixture_text_provider(&app, &model.endpoint);
    let (token, _grant_id) =
        authorize_limited_client(&app, &member, &["stylist.ask", "stylist.worker.run"]);
    let receipt = app
        .execute_as_client(
            &token,
            "stylist.ask",
            json!({"member_id":member,"operation_key":"client-write-denied","provider_id":provider,"question":"记一下今天穿了毛衣","timezone":"UTC"}),
        )
        .unwrap();
    let id = receipt["job_id"].as_str().unwrap().to_owned();
    let job = app
        .execute_as_client(
            &token,
            "stylist.worker.run",
            json!({"member_id":member,"id":id}),
        )
        .unwrap();
    assert_eq!(job["state"], "failed");
    assert_eq!(
        job["error"]["code"], "scope_denied",
        "the wear.record write is deferred under the client grant revalidation"
    );
    assert_eq!(model.post_count(), 1);
    assert_eq!(job["stylist"]["tool_calls"], json!([]));
    assert_member_has_no_wear(&app, &member);
}

#[test]
fn implicit_surfaces_never_become_writes_through_the_stylist() {
    let d = tempfile::tempdir().unwrap();
    let app = open_isolated_app(d.path());
    let member = bootstrap_member(&app);
    // Saving an outfit (or any other implicit surface) is not a whitelisted
    // tool: the loop must refuse the frame without executing anything.
    let model = ScriptedModel::start(vec![
        json!({"kind":"tool","operation":"recommendation.save","arguments":{"member_id":member,"id":"01a116f0-intent","proposal_index":0,"operation_key":"implicit-save","name":"想要这套"}}),
        json!({"kind":"answer","facts":[],"explanation":"保存工具被拒后不应到达第二轮"}),
    ]);
    let provider = save_fixture_text_provider(&app, &model.endpoint);
    let receipt = app
        .execute(
            "stylist.ask",
            json!({"member_id":member,"operation_key":"implicit-no-write","provider_id":provider,"question":"这套不错帮我留着","timezone":"UTC"}),
        )
        .unwrap();
    let id = receipt["job_id"].as_str().unwrap().to_owned();
    let job = app
        .execute("stylist.worker.run", json!({"member_id":member,"id":id}))
        .unwrap();
    assert_eq!(job["state"], "failed");
    assert_eq!(job["error"]["code"], "tool_not_allowed");
    assert_eq!(model.post_count(), 1);
    assert_eq!(job["stylist"]["tool_calls"], json!([]));
    assert_member_has_no_wear(&app, &member);
}

#[test]
fn care_record_correct_revoke_chain_grounds_through_care_list() {
    let d = tempfile::tempdir().unwrap();
    let app = open_isolated_app(d.path());
    let member = bootstrap_member(&app);
    let item = app
        .execute(
            "clothing.create",
            json!({"member_id":member,"name":"羊毛毛衣","category":"top"}),
        )
        .unwrap();
    let item_id = item["id"].as_str().unwrap().to_owned();
    let first_model = ScriptedModel::start(vec![
        json!({"kind":"tool","operation":"care.record","arguments":{"member_id":member,"clothing_id":item_id,"action":"wash","notes":"手洗晾干"}}),
        json!({"kind":"answer","facts":[{"call_id":0,"path":"/root_event_id"}],"explanation":"已记录洗护"}),
    ]);
    let provider = save_fixture_text_provider(&app, &first_model.endpoint);
    // A fresh fixture per job: the scripted replies need the durable root id,
    // which only exists after the first job finished.
    let first = app
        .execute(
            "stylist.ask",
            json!({"member_id":member,"operation_key":"care-record-explicit","provider_id":provider,"question":"这件毛衣洗过了，记一下","timezone":"UTC"}),
        )
        .unwrap();
    let first_job = first["job_id"].as_str().unwrap().to_owned();
    let done = app
        .execute(
            "stylist.worker.run",
            json!({"member_id":member,"id":first_job}),
        )
        .unwrap();
    assert_eq!(done["state"], "succeeded", "{done}");
    let root = done["result"]["tool_calls"][0]["result"]["root_event_id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        done["result"]["tool_calls"][0]["result"]["kind"],
        json!("record")
    );
    assert_eq!(
        done["result"]["tool_calls"][0]["result"]["revision"],
        json!(1)
    );
    // Correction reads the durable root from the care.list projection first.
    let correct_model = ScriptedModel::start(vec![
        json!({"kind":"tool","operation":"care.list","arguments":{"member_id":member}}),
        json!({"kind":"tool","operation":"care.correct","arguments":{"member_id":member,"root_event_id":root,"expected_revision":1,"action":"dry_clean"}}),
        json!({"kind":"answer","facts":[{"call_id":1,"path":"/revision"}],"explanation":"已更正为干洗"}),
    ]);
    let correct_provider = save_fixture_text_provider(&app, &correct_model.endpoint);
    let second = app
        .execute(
            "stylist.ask",
            json!({"member_id":member,"operation_key":"care-correct-explicit","provider_id":correct_provider,"question":"记错了，其实是干洗，更正一下","timezone":"UTC"}),
        )
        .unwrap();
    let second_job = second["job_id"].as_str().unwrap().to_owned();
    let corrected = app
        .execute(
            "stylist.worker.run",
            json!({"member_id":member,"id":second_job}),
        )
        .unwrap();
    assert_eq!(corrected["state"], "succeeded", "{corrected}");
    assert_eq!(correct_model.post_count(), 3);
    // The correction was grounded in the care.list projection: the follow-up
    // prompt carried the real root id and revision from the read.
    assert!(
        correct_model.request(1).contains(&root),
        "the correction turn must carry the real care.list projection"
    );
    assert_eq!(
        corrected["result"]["tool_calls"][1]["result"]["kind"],
        json!("correction")
    );
    assert_eq!(
        corrected["result"]["tool_calls"][1]["result"]["revision"],
        json!(2)
    );
    // Revocation against the corrected revision 2.
    let revoke_model = ScriptedModel::start(vec![
        json!({"kind":"tool","operation":"care.revoke","arguments":{"member_id":member,"root_event_id":root,"expected_revision":2,"reason":"整条记错了"}}),
        json!({"kind":"answer","facts":[{"call_id":0,"path":"/kind"}],"explanation":"已撤销该洗护记录"}),
    ]);
    let revoke_provider = save_fixture_text_provider(&app, &revoke_model.endpoint);
    let third = app
        .execute(
            "stylist.ask",
            json!({"member_id":member,"operation_key":"care-revoke-explicit","provider_id":revoke_provider,"question":"这条洗护记录整条记错了，撤销","timezone":"UTC"}),
        )
        .unwrap();
    let third_job = third["job_id"].as_str().unwrap().to_owned();
    let revoked = app
        .execute(
            "stylist.worker.run",
            json!({"member_id":member,"id":third_job}),
        )
        .unwrap();
    assert_eq!(revoked["state"], "succeeded", "{revoked}");
    assert_eq!(
        revoked["result"]["tool_calls"][0]["result"]["kind"],
        json!("revocation")
    );
    assert_eq!(
        revoked["result"]["tool_calls"][0]["result"]["revision"],
        json!(3)
    );
    // Durable truth: the public care.history carries the full correction chain
    // (record -> correction -> revocation); the default care.list hides a fully
    // revoked root by domain semantics. The wear ledger was never touched.
    let history = app
        .execute(
            "care.history",
            json!({"member_id":member,"root_event_id":root}),
        )
        .unwrap();
    let revisions = history["items"].as_array().unwrap();
    assert_eq!(revisions.len(), 3, "{history}");
    assert_eq!(revisions[0]["kind"], json!("record"));
    assert_eq!(revisions[1]["kind"], json!("correction"));
    assert_eq!(revisions[2]["kind"], json!("revocation"));
    assert_eq!(revisions[2]["revision"], json!(3));
    let care = app
        .execute(
            "care.list",
            json!({"member_id":member,"include_revoked":true}),
        )
        .unwrap();
    let visible = care["items"].as_array().unwrap();
    assert_eq!(visible.len(), 1, "{care}");
    assert_eq!(visible[0]["kind"], json!("revocation"));
    assert_member_has_no_wear(&app, &member);
}

#[test]
fn explicit_clothing_edit_and_archive_apply_server_scoped_changes() {
    let d = tempfile::tempdir().unwrap();
    let app = open_isolated_app(d.path());
    let member = bootstrap_member(&app);
    let item = app
        .execute(
            "clothing.create",
            json!({"member_id":member,"name":"羊毛毛衣","category":"top"}),
        )
        .unwrap();
    let item_id = item["id"].as_str().unwrap().to_owned();
    // Forged member and unknown fields are stripped by the server; only the
    // whitelisted fact fields reach the domain update.
    let model = ScriptedModel::start(vec![
        json!({"kind":"tool","operation":"clothing.update","arguments":{"member_id":member,"id":item_id,"name":"传家宝毛衣","favorite":true,"purchase_price_minor":1,"status":"deleted"}}),
        json!({"kind":"tool","operation":"clothing.archive","arguments":{"member_id":member,"id":item_id}}),
        json!({"kind":"answer","facts":[{"call_id":1,"path":"/status"}],"explanation":"已改名收藏并归档"}),
    ]);
    let provider = save_fixture_text_provider(&app, &model.endpoint);
    let receipt = app
        .execute(
            "stylist.ask",
            json!({"member_id":member,"operation_key":"explicit-edit-archive","provider_id":provider,"question":"这件改名传家宝毛衣、标收藏，然后归档","timezone":"UTC"}),
        )
        .unwrap();
    let id = receipt["job_id"].as_str().unwrap().to_owned();
    let job = app
        .execute("stylist.worker.run", json!({"member_id":member,"id":id}))
        .unwrap();
    assert_eq!(job["state"], "succeeded", "{job}");
    let tool_calls = job["result"]["tool_calls"].as_array().unwrap();
    assert_eq!(tool_calls.len(), 2);
    assert_eq!(tool_calls[0]["result"]["name"], json!("传家宝毛衣"));
    assert_eq!(tool_calls[0]["result"]["favorite"], json!(true));
    assert_eq!(tool_calls[1]["result"]["status"], json!("archived"));
    assert_eq!(job["result"]["facts"][0]["value"], json!("archived"));
    // Durable read-back through the public surface: the edits persisted, the
    // forged fields never reached the document.
    let stored = app
        .execute("clothing.get", json!({"member_id":member,"id":item_id}))
        .unwrap();
    assert_eq!(stored["name"], json!("传家宝毛衣"));
    assert_eq!(stored["favorite"], json!(true));
    assert_eq!(stored["status"], json!("archived"));
    assert_eq!(
        stored["purchase_price_minor"], item["purchase_price_minor"],
        "a forged price must not reach the domain update"
    );
    assert_eq!(
        job["result"]["budget"],
        json!({"model_turns":3,"tool_calls":2})
    );
    assert_member_has_no_wear(&app, &member);
}

fn facts_of<'a>(job: &'a Value) -> &'a Vec<Value> {
    job["result"]["facts"].as_array().expect("facts array")
}

#[test]
fn answer_citing_unprojected_notes_exhausts_invalid_budget_without_leaking_sentinel() {
    let d = tempfile::tempdir().unwrap();
    let app = open_isolated_app(d.path());
    let member = bootstrap_member(&app);
    const SENTINEL: &str = "机密备注哨兵-9f2c-private-notes";
    let item = app
        .execute(
            "clothing.create",
            json!({"member_id":member,"name":"羊毛毛衣","category":"top","notes":SENTINEL}),
        )
        .unwrap();
    // A real tool read happened, but the answer cites a pointer that the server
    // never projected: /items/0/notes. Repeated initial plus two retries.
    let invalid_answer = json!({"kind":"answer","facts":[{"call_id":0,"path":"/items/0/notes"}],"explanation":"引用未经投影的备注"});
    let model = ScriptedModel::start(vec![
        json!({"kind":"tool","operation":"clothing.list","arguments":{"member_id":member,"limit":24}}),
        invalid_answer.clone(),
        invalid_answer.clone(),
        invalid_answer,
    ]);
    let provider = save_fixture_text_provider(&app, &model.endpoint);
    let receipt = app
        .execute(
            "stylist.ask",
            json!({"member_id":member,"operation_key":"unprojected-notes","provider_id":provider,"question":"今天穿什么","timezone":"UTC"}),
        )
        .unwrap();
    let id = receipt["job_id"].as_str().unwrap().to_owned();
    let job = app
        .execute("stylist.worker.run", json!({"member_id":member,"id":id}))
        .unwrap();
    assert_eq!(job["state"], "failed");
    assert_eq!(job["error"]["code"], "model_invalid_output");
    assert_eq!(
        model.post_count(),
        4,
        "tool request plus three invalid answers: the initial attempt and two retries"
    );
    assert!(
        job["result"].is_null(),
        "a failed invalid-output loop must not publish any result facts"
    );
    for index in 0..model.post_count() {
        assert!(
            !model.request(index).contains(SENTINEL),
            "the notes sentinel must never reach model prompt {index}"
        );
    }
    // The durable projection that fed the retry prompts carries only safe fields.
    let stored = app
        .execute("jobs.get", json!({"member_id":member,"id":id}))
        .unwrap();
    let projected_item = &stored["stylist"]["tool_calls"][0]["result"]["items"][0];
    assert_eq!(
        projected_item["id"], item["id"],
        "the projected tool result still grounded the real item id"
    );
    assert!(
        projected_item
            .as_object()
            .is_some_and(|fields| !fields.contains_key("notes")),
        "the projected tool result must not carry a notes field at all"
    );
    assert_member_has_no_wear(&app, &member);
}

// #68：真实模型请求必须产生全链路日志事件——每次发送记一条 provider_request，
// trace_id 与任务文档一致，字段白名单脱敏（与 diagnostics 的供应商事件同一套构造）。
#[test]
fn real_model_sends_log_provider_request_events_on_the_job_trace() {
    let d = tempfile::tempdir().unwrap();
    let app = Application::open(&d.path().join("db")).unwrap();
    let member = app.execute("space.default", json!({})).unwrap()["member_id"]
        .as_str()
        .unwrap()
        .to_string();
    app.execute(
        "clothing.create",
        json!({"member_id":member,"name":"羊毛毛衣","category":"top"}),
    )
    .unwrap();
    let model = ScriptedModel::start(vec![
        json!({"kind":"tool","operation":"clothing.list","arguments":{"member_id":member,"limit":24}}),
        json!({"kind":"answer","facts":[{"call_id":0,"path":"/items/0/id"}],"explanation":"以上是当前可用衣物"}),
    ]);
    let provider = save_fixture_text_provider(&app, &model.endpoint);
    let receipt = app
        .execute(
            "stylist.ask",
            json!({"member_id":member,"operation_key":"provider-log-loop","provider_id":provider,"question":"今天穿什么","timezone":"UTC"}),
        )
        .unwrap();
    let id = receipt["job_id"].as_str().unwrap().to_owned();
    let job = app
        .execute("stylist.worker.run", json!({"member_id":member,"id":id}))
        .unwrap();
    assert_eq!(job["state"], "succeeded", "{job}");
    assert_eq!(model.post_count(), 2, "两次真实模型发送");

    let stored = app
        .execute("jobs.get", json!({"member_id":member,"id":id}))
        .unwrap();
    let trace = stored["trace_id"]
        .as_str()
        .expect("任务文档携带 trace_id")
        .to_string();
    let queried = app
        .execute(
            "diagnostics.logs.query",
            json!({"trace_id":trace,"operation":"provider.","limit":100}),
        )
        .unwrap();
    let events: Vec<&Value> = queried["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["event_type"] == "provider_request")
        .collect();
    assert_eq!(
        events.len(),
        2,
        "每次真实模型发送各记一条事件（此前为零）：{queried}"
    );
    for event in &events {
        assert_eq!(event["operation"], "provider.request");
        assert_eq!(event["endpoint_kind"], "chat_completions");
        assert_eq!(event["model"], "fixture-text");
        assert_eq!(event["outcome"], "ok");
        assert_eq!(event["trace_id"], json!(trace), "事件落在任务文档的 trace 上");
        assert!(event["duration_ms"].is_number());
        // 脱敏按构造：事件只含白名单字段，密钥/请求头/请求与响应体没有容身之处
        let allowed = [
            "ts",
            "level",
            "event_type",
            "run_id",
            "trace_id",
            "operation",
            "endpoint_kind",
            "model",
            "outcome",
            "duration_ms",
        ];
        for key in event.as_object().unwrap().keys() {
            assert!(
                allowed.contains(&key.as_str()),
                "供应商请求事件出现白名单外字段 {key}：{event}"
            );
        }
    }
}

// #68：stylist 限流重试环的每次真实发送同样要留一条事件——429 记 http_error
// 与 HTTP 状态，任务按 Retry-After 排队等待到期重试。
#[test]
fn rate_limited_model_send_logs_http_error_event_before_deferring() {
    let d = tempfile::tempdir().unwrap();
    let app = Application::open(&d.path().join("db")).unwrap();
    let member = app.execute("space.default", json!({})).unwrap()["member_id"]
        .as_str()
        .unwrap()
        .to_string();
    let model = ScriptedModel::start_turns(vec![ScriptedTurn::Status(429)]);
    let provider = save_fixture_text_provider(&app, &model.endpoint);
    let receipt = app
        .execute(
            "stylist.ask",
            json!({"member_id":member,"operation_key":"provider-log-rate","provider_id":provider,"question":"今天穿什么","timezone":"UTC"}),
        )
        .unwrap();
    let id = receipt["job_id"].as_str().unwrap().to_owned();
    let job = app
        .execute("stylist.worker.run", json!({"member_id":member,"id":id}))
        .unwrap();
    assert_eq!(job["state"], "queued", "首次 429 按 Retry-After 排队：{job}");
    assert_eq!(model.post_count(), 1, "恰好一次真实发送");

    let trace = job["trace_id"]
        .as_str()
        .expect("任务文档携带 trace_id")
        .to_string();
    let queried = app
        .execute(
            "diagnostics.logs.query",
            json!({"trace_id":trace,"operation":"provider.","limit":100}),
        )
        .unwrap();
    let events: Vec<&Value> = queried["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["event_type"] == "provider_request")
        .collect();
    assert_eq!(events.len(), 1, "限流发送也各记一条事件：{queried}");
    assert_eq!(events[0]["outcome"], "http_error");
    assert_eq!(events[0]["http_status"], 429);
    assert_eq!(events[0]["endpoint_kind"], "chat_completions");
    assert_eq!(events[0]["trace_id"], json!(trace));
}
