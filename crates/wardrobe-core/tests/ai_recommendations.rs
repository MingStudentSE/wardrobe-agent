use serde_json::{json, Value};
use wardrobe_core::Application;
// Public client completion protocol: enqueue -> explicit member-scoped worker -> query intent.
// Initial async receipt is tested independently below without this helper.
trait CompletionClient {
    fn complete(&self, op: &str, input: Value) -> Result<Value, wardrobe_core::ApiError>;
}
impl CompletionClient for Application {
    fn complete(&self, op: &str, input: Value) -> Result<Value, wardrobe_core::ApiError> {
        let ai = op == "recommendation.generate" && input["strategy"] == "ai";
        let member = input["member_id"].clone();
        let receipt = self.execute(op, input)?;
        if !ai {
            return Ok(receipt);
        }
        if receipt["intent_id"].is_string() {
            return self.execute(
                "recommendation.get",
                json!({"member_id":member,"id":receipt["intent_id"]}),
            );
        }
        let job = self.execute(
            "recommendation.worker.run",
            json!({"member_id":member,"id":receipt["job_id"]}),
        )?;
        if job["result"]["intent_id"].is_string() {
            return self.execute(
                "recommendation.get",
                json!({"member_id":member,"id":job["result"]["intent_id"]}),
            );
        }
        Err(wardrobe_core::ApiError::of("generation_waiting_input".to_owned(), job.to_string()))
    }
}
fn seed(a: &Application) -> Vec<Value> {
    ["top", "bottom", "shoes"]
        .into_iter()
        .map(|c| {
            a.complete(
                "clothing.create",
                json!({"member_id":"default","name":c,"category":c}),
            )
            .unwrap()
        })
        .collect()
}
#[test]
fn illegal_model_ids_retry_twice_then_real_rule_fallback_preserves_fixed_and_no_wear() {
    let server = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}/v1", server.local_addr().unwrap());
    let worker = std::thread::spawn(move || {
        use std::io::{Read, Write};
        for _ in 0..2 {
            let (mut s, _) = server.accept().unwrap();
            let mut b = [0; 65536];
            let n = s.read(&mut b).unwrap();
            assert!(String::from_utf8_lossy(&b[..n]).contains("/chat/completions"));
            let content=json!({"proposals":[{"items":[{"clothing_id":"fabricated-id","role":"body","layer":0}]}]}).to_string();
            let body = json!({"choices":[{"message":{"content":content}}]}).to_string();
            write!(
                s,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
        }
    });
    let d = tempfile::tempdir().unwrap();
    let a = Application::open(&d.path().join("db")).unwrap();
    let stock = seed(&a);
    let p=a.complete("provider.save",json!({"name":"protocol only","endpoint":endpoint,"model":"configured-text","capabilities":["text"],"authentication":"none"})).unwrap();
    let r=a.complete("recommendation.generate",json!({"member_id":"default","operation_key":"ai","timezone":"UTC","strategy":"ai","provider_id":p["id"],"fixed_ids":[stock[0]["id"]]})).unwrap();
    worker.join().unwrap();
    assert_eq!(r["generation"]["source"], "rule_fallback");
    assert_eq!(r["generation"]["attempts"], 2);
    assert_eq!(r["actual_count"], 1);
    assert!(r["proposals"][0]["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|i| i["clothing_id"] == stock[0]["id"]));
    assert_eq!(
        a.complete("wear.list", json!({"member_id":"default"}))
            .unwrap()["items"],
        json!([])
    );
    let retry = a
        .complete("recommendation.generate", r["controls"].clone())
        .unwrap();
    assert_eq!(retry, r);
}
#[test]
fn model_cannot_claim_recent_repeat_is_inventory_shortage_when_fresh_alternative_exists() {
    let d = tempfile::tempdir().unwrap();
    let a = Application::open(&d.path().join("db")).unwrap();
    let stock = seed(&a);
    a.complete(
        "clothing.create",
        json!({"member_id":"default","name":"fresh top","category":"top"}),
    )
    .unwrap();
    a.complete("wear.record",json!({"member_id":"default","operation_key":"wear","local_date":chrono::Utc::now().date_naive().to_string(),"timezone":"UTC","clothing_ids":stock.iter().map(|c|c["id"].clone()).collect::<Vec<_>>()})).unwrap();
    let server = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}/v1", server.local_addr().unwrap());
    let items: Vec<Value> = stock
        .iter()
        .map(|c| json!({"clothing_id":c["id"],"role":c["category"],"layer":0}))
        .collect();
    let worker = std::thread::spawn(move || {
        use std::io::{Read, Write};
        for _ in 0..2 {
            let (mut s, _) = server.accept().unwrap();
            let mut b = [0; 65536];
            assert!(s.read(&mut b).unwrap() > 0);
            let body=json!({"choices":[{"message":{"content":json!({"proposals":[{"items":items}]}).to_string()}}]}).to_string();
            write!(
                s,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
        }
    });
    let p=a.complete("provider.save",json!({"name":"repeat protocol","endpoint":endpoint,"model":"text","capabilities":["text"],"authentication":"none"})).unwrap();
    let r=a.complete("recommendation.generate",json!({"member_id":"default","operation_key":"recent-ai","timezone":"UTC","limit":1,"strategy":"ai","provider_id":p["id"]})).unwrap();
    assert_eq!(r["generation"]["source"], "rule_fallback");
    worker.join().unwrap();
    assert_eq!(r["proposals"][0]["repeat_relaxed"], false);
}
#[test]
fn valid_model_outfit_uses_current_real_ids_and_shared_template_save_without_wear() {
    let d = tempfile::tempdir().unwrap();
    let a = Application::open(&d.path().join("db")).unwrap();
    let stock = seed(&a);
    let server = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}/v1", server.local_addr().unwrap());
    let items: Vec<Value> = stock
        .iter()
        .map(|c| json!({"clothing_id":c["id"],"role":c["category"],"layer":0}))
        .collect();
    let worker = std::thread::spawn(move || {
        use std::io::{Read, Write};
        let (mut s, _) = server.accept().unwrap();
        let mut b = [0; 65536];
        assert!(s.read(&mut b).unwrap() > 0);
        let body=json!({"choices":[{"message":{"content":json!({"proposals":[{"items":items}]}).to_string()}}]}).to_string();
        write!(
            s,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        )
        .unwrap();
    });
    let p=a.complete("provider.save",json!({"name":"valid protocol","endpoint":endpoint,"model":"text","capabilities":["text"],"authentication":"none"})).unwrap();
    let r=a.complete("recommendation.generate",json!({"member_id":"default","operation_key":"valid-ai","timezone":"UTC","limit":1,"strategy":"ai","provider_id":p["id"],"fixed_ids":[stock[0]["id"]]})).unwrap();
    worker.join().unwrap();
    assert_eq!(r["generation"]["source"], "text_model");
    assert_eq!(r["generation"]["attempts"], 1);
    assert_eq!(
        a.complete("recommendation.generate", r["controls"].clone())
            .unwrap(),
        r
    );
    a.complete("recommendation.save",json!({"member_id":"default","id":r["id"],"proposal_index":0,"name":"actual validated","operation_key":"save"})).unwrap();
    assert_eq!(
        a.complete("wear.list", json!({"member_id":"default"}))
            .unwrap()["items"],
        json!([])
    );
    assert_eq!(
        a.complete("outfit.list", json!({"member_id":"default"}))
            .unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}
#[test]
fn model_response_after_inventory_changes_cannot_restore_archived_item() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("db");
    let a = Application::open(&path).unwrap();
    let stock = seed(&a);
    let fresh = a
        .complete(
            "clothing.create",
            json!({"member_id":"default","name":"fresh top","category":"top"}),
        )
        .unwrap();
    let server = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}/v1", server.local_addr().unwrap());
    let old = stock[0]["id"].clone();
    let items: Vec<Value> = stock
        .iter()
        .map(|c| json!({"clothing_id":c["id"],"role":c["category"],"layer":0}))
        .collect();
    let worker = std::thread::spawn(move || {
        use std::io::{Read, Write};
        for i in 0..2 {
            let (mut s, _) = server.accept().unwrap();
            let mut b = [0; 65536];
            assert!(s.read(&mut b).unwrap() > 0);
            if i == 0 {
                let other = Application::open(&path).unwrap();
                other
                    .complete("clothing.archive", json!({"member_id":"default","id":old}))
                    .unwrap();
            }
            let body=json!({"choices":[{"message":{"content":json!({"proposals":[{"items":items}]}).to_string()}}]}).to_string();
            write!(
                s,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
        }
    });
    let p=a.complete("provider.save",json!({"name":"late protocol","endpoint":endpoint,"model":"text","capabilities":["text"],"authentication":"none"})).unwrap();
    let r=a.complete("recommendation.generate",json!({"member_id":"default","operation_key":"late-ai","timezone":"UTC","limit":1,"strategy":"ai","provider_id":p["id"]})).unwrap();
    worker.join().unwrap();
    assert_eq!(r["generation"]["source"], "rule_fallback");
    assert!(r["proposals"][0]["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|i| i["clothing_id"] == fresh["id"]));
    assert!(!r["proposals"][0]["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|i| i["clothing_id"] == stock[0]["id"]));
}
#[test]
fn two_public_connections_same_intent_have_only_one_provider_submission() {
    use std::io::{Read, Write};
    use std::sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc, Arc,
    };
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("db");
    let a = Application::open(&path).unwrap();
    let stock = seed(&a);
    let server = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    server.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}/v1", server.local_addr().unwrap());
    let items: Vec<Value> = stock
        .iter()
        .map(|c| json!({"clothing_id":c["id"],"role":c["category"],"layer":0}))
        .collect();
    let body=json!({"choices":[{"message":{"content":json!({"proposals":[{"items":items}]}).to_string()}}]}).to_string();
    let count = Arc::new(AtomicUsize::new(0));
    let stop = Arc::new(AtomicBool::new(false));
    let c = count.clone();
    let stopped = stop.clone();
    let (seen, received) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let gate = Arc::new(std::sync::Mutex::new(gate));
    let listener = std::thread::spawn(move || {
        let mut workers = vec![];
        while !stopped.load(Ordering::SeqCst) {
            match server.accept() {
                Ok((mut s, _)) => {
                    s.set_nonblocking(false).unwrap();
                    let c = c.clone();
                    let seen = seen.clone();
                    let gate = gate.clone();
                    let body = body.clone();
                    workers.push(std::thread::spawn(move || {
                        let mut b = [0; 65536];
                        assert!(s.read(&mut b).unwrap() > 0);
                        let n = c.fetch_add(1, Ordering::SeqCst);
                        if n == 0 {
                            seen.send(()).unwrap();
                            gate.lock()
                                .unwrap()
                                .recv_timeout(std::time::Duration::from_secs(10))
                                .unwrap();
                        }
                        write!(
                            s,
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            body.len(),
                            body
                        )
                        .unwrap();
                    }));
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(std::time::Duration::from_millis(1))
                }
                Err(e) => panic!("{e}"),
            }
        }
        for w in workers {
            w.join().unwrap();
        }
    });
    let p=a.complete("provider.save",json!({"name":"race protocol","endpoint":endpoint,"model":"text","capabilities":["text"],"authentication":"none"})).unwrap();
    let input = json!({"member_id":"default","operation_key":"same-race","timezone":"UTC","limit":1,"strategy":"ai","provider_id":p["id"]});
    drop(a);
    let first_path = path.clone();
    let first_input = input.clone();
    let first = std::thread::spawn(move || {
        Application::open(&first_path)
            .unwrap()
            .complete("recommendation.generate", first_input)
    });
    received
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap();
    let second = Application::open(&path)
        .unwrap()
        .complete("recommendation.generate", input);
    release.send(()).unwrap();
    let result = first.join().unwrap();
    stop.store(true, Ordering::SeqCst);
    listener.join().unwrap();
    assert_eq!(
        count.load(Ordering::SeqCst),
        1,
        "same logical paid request must not submit twice"
    );
    assert!(second.is_err());
    assert_eq!(result.unwrap()["generation"]["source"], "text_model");
}

#[test]
fn cancelled_or_expired_submitted_request_cannot_publish_or_retry() {
    for (cancel, invalid_response) in [(true, false), (true, true), (false, false)] {
        use std::io::{Read, Write};
        let d = tempfile::tempdir().unwrap();
        let path = d.path().join("db");
        let a = Application::open(&path).unwrap();
        let stock = seed(&a);
        let server = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}/v1", server.local_addr().unwrap());
        let (seen, received) = std::sync::mpsc::channel();
        let (release, gate) = std::sync::mpsc::channel();
        let content = if invalid_response {
            "not JSON".to_owned()
        } else {
            json!({"proposals":[{"items":stock.iter().map(|c|json!({"clothing_id":c["id"],"role":c["category"],"layer":0})).collect::<Vec<_>>()}]}).to_string()
        };
        let body = json!({"choices":[{"message":{"content":content}}]}).to_string();
        let worker = std::thread::spawn(move || {
            let (mut s, _) = server.accept().unwrap();
            let mut b = [0; 65536];
            assert!(s.read(&mut b).unwrap() > 0);
            seen.send(()).unwrap();
            gate.recv_timeout(std::time::Duration::from_secs(10))
                .unwrap();
            write!(
                s,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
        });
        let p=a.complete("provider.save",json!({"name":"lease protocol","endpoint":endpoint,"model":"text","capabilities":["text"],"authentication":"none"})).unwrap();
        let input = json!({"member_id":"default","operation_key":"lease","timezone":"UTC","limit":1,"strategy":"ai","provider_id":p["id"]});
        let request = input.clone();
        let worker_path = path.clone();
        let generation = std::thread::spawn(move || {
            Application::open(&worker_path)
                .unwrap()
                .complete("recommendation.generate", request)
        });
        received
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        let connection = rusqlite::Connection::open(&path).unwrap();
        let raw: String = connection
            .query_row(
                "SELECT document FROM jobs WHERE kind='text_recommendation'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let mut job: Value = serde_json::from_str(&raw).unwrap();
        let args = json!({"member_id":"default","id":job["id"]});
        if cancel {
            a.complete("jobs.cancel", args.clone()).unwrap();
        } else {
            // Clock seam only: the submitted provider reply remains a real blocked TCP response.
            job["lease_expires"] = json!(chrono::Utc::now().timestamp_millis() - 1);
            connection
                .execute(
                    "UPDATE jobs SET document=?1 WHERE id=?2",
                    rusqlite::params![job.to_string(), job["id"].as_str().unwrap()],
                )
                .unwrap();
        }
        release.send(()).unwrap();
        assert!(generation.join().unwrap().is_err());
        worker.join().unwrap();
        let count: i64 = connection
            .query_row("SELECT count(*) FROM recommendation_intents", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(
            count, 0,
            "late cancelled/expired result cannot publish an intent"
        );
        assert_eq!(
            a.complete("jobs.get", args.clone()).unwrap()["submission_count"],
            1,
            "cancellation of a confirmed invalid first response must not grant a second submission"
        );
        assert_eq!(
            a.complete("jobs.retry", args.clone()).unwrap_err().code,
            "submission_already_sent"
        );
        assert_eq!(
            a.complete("recommendation.generate", input)
                .unwrap_err()
                .code,
            "generation_waiting_input"
        );
        if !cancel {
            assert_eq!(
                a.complete("jobs.get", args).unwrap()["state"],
                "waiting_input"
            );
        }
    }
}

#[test]
fn confirmed_rate_limit_waits_retry_after_then_returns_valid_model_result() {
    use std::io::{Read, Write};
    let d = tempfile::tempdir().unwrap();
    let a = Application::open(&d.path().join("db")).unwrap();
    let stock = seed(&a);
    let items: Vec<Value> = stock
        .iter()
        .map(|c| json!({"clothing_id":c["id"],"role":c["category"],"layer":0}))
        .collect();
    let server = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    server.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}/v1", server.local_addr().unwrap());
    let worker = std::thread::spawn(move || {
        let start = std::time::Instant::now();
        let mut received = vec![];
        while start.elapsed() < std::time::Duration::from_secs(4) && received.len() < 2 {
            let (mut s, _) = match server.accept() {
                Ok(v) => v,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(std::time::Duration::from_millis(5));
                    continue;
                }
                Err(e) => panic!("{e}"),
            };
            s.set_nonblocking(false).unwrap();
            s.set_read_timeout(Some(std::time::Duration::from_secs(2)))
                .unwrap();
            let mut b = [0; 65536];
            assert!(s.read(&mut b).unwrap() > 0);
            received.push(std::time::Instant::now());
            if received.len() == 1 {
                write!(s,"HTTP/1.1 429 Too Many Requests\r\nRetry-After: 1\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
            } else {
                let body=json!({"choices":[{"message":{"content":json!({"proposals":[{"items":items}]}).to_string()}}]}).to_string();
                write!(
                    s,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                )
                .unwrap();
            }
        }
        received
    });
    let p=a.complete("provider.save",json!({"name":"confirmed rate limit protocol","endpoint":endpoint,"model":"text","capabilities":["text"],"authentication":"none"})).unwrap();
    let r=a.complete("recommendation.generate",json!({"member_id":"default","operation_key":"rate-limit","timezone":"UTC","limit":1,"strategy":"ai","provider_id":p["id"]})).unwrap();
    let received = worker.join().unwrap();
    assert_eq!(received.len(), 2, "confirmed rejection must be retried");
    assert!(
        received[1].duration_since(received[0]) >= std::time::Duration::from_secs(1),
        "Retry-After must be observed"
    );
    assert_eq!(r["generation"]["source"], "text_model");
    assert_eq!(
        a.complete("wear.list", json!({"member_id":"default"}))
            .unwrap()["items"],
        json!([])
    );
}

#[test]
fn initial_ai_receipt_is_durable_and_precedes_weather_or_model_http() {
    use std::io::{Read, Write};
    use std::sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    };
    let d = tempfile::tempdir().unwrap();
    let a = Application::open(&d.path().join("db")).unwrap();
    seed(&a);
    let server = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    server.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}/forecast", server.local_addr().unwrap());
    let calls = Arc::new(AtomicUsize::new(0));
    let model_calls = Arc::new(AtomicUsize::new(0));
    let mc = model_calls.clone();
    let stop = Arc::new(AtomicBool::new(false));
    let c = calls.clone();
    let done = stop.clone();
    let weather = std::thread::spawn(move || {
        while !done.load(Ordering::SeqCst) {
            match server.accept() {
                Ok((mut s, _)) => {
                    s.set_nonblocking(false).unwrap();
                    let mut b = [0; 4096];
                    assert!(s.read(&mut b).unwrap() > 0);
                    if b[..4] == *b"POST" {
                        mc.fetch_add(1, Ordering::SeqCst);
                        write!(s,"HTTP/1.1 503 Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
                        continue;
                    }
                    c.fetch_add(1, Ordering::SeqCst);
                    std::thread::sleep(std::time::Duration::from_millis(400));
                    let body=json!({"current_units":{"time":"unixtime","temperature_2m":"°C"},"current":{"time":chrono::Utc::now().timestamp(),"temperature_2m":20.0}}).to_string();
                    write!(
                        s,
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    )
                    .unwrap();
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(std::time::Duration::from_millis(2))
                }
                Err(e) => panic!("{e}"),
            }
        }
    });
    a.execute("weather.configure",json!({"member_id":"default","mode":"automatic","location":"public protocol","latitude":31.23,"longitude":121.47,"endpoint":endpoint,"usage":"noncommercial"})).unwrap();
    let p=a.execute("provider.save",json!({"name":"offline text","endpoint":endpoint.replace("/forecast","/v1"),"model":"text","capabilities":["text"],"authentication":"none"})).unwrap();
    let start = std::time::Instant::now();
    let receipt=a.execute("recommendation.generate",json!({"member_id":"default","operation_key":"immediate","timezone":"UTC","limit":1,"strategy":"ai","provider_id":p["id"],"weather":{"mode":"automatic"}})).unwrap();
    let elapsed = start.elapsed();
    let initial_calls = calls.load(Ordering::SeqCst);
    let initial_model = model_calls.load(Ordering::SeqCst);
    let durable = receipt["job_id"].as_str().map(|id| {
        Application::open(&d.path().join("db"))
            .unwrap()
            .execute("jobs.get", json!({"member_id":"default","id":id}))
            .unwrap()
    });
    let completed = receipt["job_id"].as_str().map(|id| {
        a.execute(
            "recommendation.worker.run",
            json!({"member_id":"default","id":id}),
        )
        .unwrap()
    });
    stop.store(true, Ordering::SeqCst);
    weather.join().unwrap();
    eprintln!(
        "INITIAL elapsed_us={} weather_http={} model_posts={} durable_id={}",
        elapsed.as_micros(),
        initial_calls,
        initial_model,
        receipt["job_id"]
    );
    assert_eq!(initial_model, 0);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(model_calls.load(Ordering::SeqCst), 1);
    assert!(
        elapsed < std::time::Duration::from_millis(200),
        "initial receipt took {elapsed:?} before returning its durable ID"
    );
    assert_eq!(
        initial_calls, 0,
        "initial receipt must precede even automatic weather HTTP"
    );
    assert_eq!(durable.unwrap()["state"], "queued");
    assert!(completed.unwrap()["result"]["intent_id"].is_string());
}

struct RetryProtocol {
    endpoint: String,
    posts: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    worker: Option<std::thread::JoinHandle<()>>,
}
impl RetryProtocol {
    fn start(statuses: Vec<u16>) -> Self {
        Self::with_delay(statuses, 31)
    }
    fn with_delay(statuses: Vec<u16>, delay: u64) -> Self {
        use std::io::{Read, Write};
        use std::sync::{
            atomic::{AtomicBool, AtomicUsize, Ordering},
            Arc,
        };
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = format!("http://{}/v1", listener.local_addr().unwrap());
        let posts = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let p = posts.clone();
        let done = stop.clone();
        let worker = std::thread::spawn(move || {
            while !done.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((mut s, _)) => {
                        s.set_nonblocking(false).unwrap();
                        s.set_read_timeout(Some(std::time::Duration::from_secs(2)))
                            .unwrap();
                        let mut b = [0; 65536];
                        assert!(s.read(&mut b).unwrap() > 0);
                        let n = p.fetch_add(1, Ordering::SeqCst);
                        if statuses.get(n).copied().unwrap_or(200) == 429 {
                            write!(s,"HTTP/1.1 429 Too Many Requests\r\nRetry-After: {delay}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
                        } else {
                            let body =
                                json!({"choices":[{"message":{"content":"not JSON"}}]}).to_string();
                            write!(s,"HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).unwrap();
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(std::time::Duration::from_millis(2))
                    }
                    Err(e) => panic!("{e}"),
                }
            }
        });
        Self {
            endpoint,
            posts,
            stop,
            worker: Some(worker),
        }
    }
}
impl Drop for RetryProtocol {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::SeqCst);
        self.worker.take().unwrap().join().unwrap();
    }
}
fn due_clock_seam(path: &std::path::Path, id: &str) {
    let db = rusqlite::Connection::open(path).unwrap();
    let raw: String = db
        .query_row("SELECT document FROM jobs WHERE id=?1", [id], |r| r.get(0))
        .unwrap();
    let mut job: Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(job["state"], "queued");
    assert_eq!(job["last_outcome"], "rate_limited");
    job["next_due"] = json!(0);
    db.execute(
        "UPDATE jobs SET document=?1 WHERE id=?2",
        rusqlite::params![job.to_string(), id],
    )
    .unwrap();
}
#[test]
fn invalid_output_budget_survives_reopened_worker_and_separate_rate_limit_delay() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("db");
    let a = Application::open(&path).unwrap();
    seed(&a);
    let protocol = RetryProtocol::start(vec![200, 429, 200]);
    let p=a.execute("provider.save",json!({"name":"durable invalid budget","endpoint":protocol.endpoint,"model":"text","capabilities":["text"],"authentication":"none"})).unwrap();
    let receipt=a.execute("recommendation.generate",json!({"member_id":"default","operation_key":"durable-invalid","timezone":"UTC","limit":1,"strategy":"ai","provider_id":p["id"]})).unwrap();
    let id = receipt["job_id"].as_str().unwrap();
    let args = json!({"member_id":"default","id":id});
    let queued = a
        .execute("recommendation.worker.run", args.clone())
        .unwrap();
    assert_eq!(queued["state"], "queued");
    assert_eq!(queued["output_attempt"], 2);
    assert_eq!(queued["submission_count"], 2);
    assert!(queued["next_due"].as_i64().unwrap() > chrono::Utc::now().timestamp_millis() + 30000);
    drop(a);
    due_clock_seam(&path, id);
    let reopened = Application::open(&path).unwrap();
    let done = reopened
        .execute("recommendation.worker.run", args.clone())
        .unwrap();
    assert_eq!(done["state"], "succeeded");
    assert_eq!(done["output_attempt"], 2);
    assert_eq!(done["rate_retries"], 1);
    assert_eq!(done["submission_count"], 3);
    let intent = reopened
        .execute(
            "recommendation.get",
            json!({"member_id":"default","id":done["result"]["intent_id"]}),
        )
        .unwrap();
    assert_eq!(intent["generation"]["source"], "rule_fallback");
    assert_eq!(intent["generation"]["attempts"], 2);
    assert_eq!(
        reopened.execute("recommendation.worker.run", args).unwrap(),
        done
    );
    assert_eq!(protocol.posts.load(std::sync::atomic::Ordering::SeqCst), 3);
}
#[test]
fn confirmed_rate_limit_budget_is_global_across_three_reopened_workers() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("db");
    let a = Application::open(&path).unwrap();
    seed(&a);
    let protocol = RetryProtocol::start(vec![429, 429, 429]);
    let p=a.execute("provider.save",json!({"name":"durable rate budget","endpoint":protocol.endpoint,"model":"text","capabilities":["text"],"authentication":"none"})).unwrap();
    let receipt=a.execute("recommendation.generate",json!({"member_id":"default","operation_key":"durable-rate","timezone":"UTC","limit":1,"strategy":"ai","provider_id":p["id"]})).unwrap();
    let id = receipt["job_id"].as_str().unwrap();
    drop(a);
    for index in 0..3 {
        let a = Application::open(&path).unwrap();
        let job = a
            .execute(
                "recommendation.worker.run",
                json!({"member_id":"default","id":id}),
            )
            .unwrap();
        assert_eq!(job["submission_count"], index + 1);
        if index < 2 {
            assert_eq!(job["state"], "queued");
            drop(a);
            due_clock_seam(&path, id);
        } else {
            assert_eq!(job["state"], "succeeded");
            assert_eq!(job["rate_retries"], 2);
            assert!(job["result"]["intent_id"].is_string());
        }
    }
    assert_eq!(protocol.posts.load(std::sync::atomic::Ordering::SeqCst), 3);
}

#[test]
fn cancellation_or_expiry_during_retry_after_wait_prevents_second_post() {
    for cancel in [true, false] {
        let d = tempfile::tempdir().unwrap();
        let path = d.path().join("db");
        let a = Application::open(&path).unwrap();
        seed(&a);
        let protocol = RetryProtocol::with_delay(vec![429], 1);
        let p=a.execute("provider.save",json!({"name":"cancel retry wait","endpoint":protocol.endpoint,"model":"text","capabilities":["text"],"authentication":"none"})).unwrap();
        let receipt=a.execute("recommendation.generate",json!({"member_id":"default","operation_key":"cancel-wait","timezone":"UTC","limit":1,"strategy":"ai","provider_id":p["id"]})).unwrap();
        let id = receipt["job_id"].as_str().unwrap().to_owned();
        let args = json!({"member_id":"default","id":id});
        let worker_args = args.clone();
        let worker_path = path.clone();
        let worker = std::thread::spawn(move || {
            Application::open(&worker_path)
                .unwrap()
                .execute("recommendation.worker.run", worker_args)
        });
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        loop {
            let job = a.execute("jobs.get", args.clone()).unwrap();
            if job["last_outcome"] == "rate_limited" {
                break;
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        if cancel {
            a.execute("jobs.cancel", args.clone()).unwrap();
        } else {
            let db = rusqlite::Connection::open(&path).unwrap();
            let mut job = a.execute("jobs.get", args.clone()).unwrap();
            job["lease_expires"] = json!(chrono::Utc::now().timestamp_millis() - 1);
            db.execute(
                "UPDATE jobs SET document=?1 WHERE id=?2",
                rusqlite::params![job.to_string(), id],
            )
            .unwrap();
        }
        worker.join().unwrap().unwrap();
        let job = a.execute("jobs.get", args).unwrap();
        assert_eq!(job["submission_count"], 1);
        assert_eq!(protocol.posts.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert!(job["result"]["intent_id"].is_null());
    }
}
