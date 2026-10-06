use serde_json::{json, Value};
use wardrobe_core::Application;
fn seed(a: &Application) -> Vec<Value> {
    ["top", "bottom", "shoes"]
        .into_iter()
        .map(|c| {
            a.execute(
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
    let p=a.execute("provider.save",json!({"name":"protocol only","endpoint":endpoint,"model":"configured-text","capabilities":["text"],"authentication":"none"})).unwrap();
    let r=a.execute("recommendation.generate",json!({"member_id":"default","operation_key":"ai","timezone":"UTC","strategy":"ai","provider_id":p["id"],"fixed_ids":[stock[0]["id"]]})).unwrap();
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
        a.execute("wear.list", json!({"member_id":"default"}))
            .unwrap()["items"],
        json!([])
    );
    let retry = a
        .execute("recommendation.generate", r["controls"].clone())
        .unwrap();
    assert_eq!(retry, r);
}
#[test]
fn model_cannot_claim_recent_repeat_is_inventory_shortage_when_fresh_alternative_exists() {
    let d = tempfile::tempdir().unwrap();
    let a = Application::open(&d.path().join("db")).unwrap();
    let stock = seed(&a);
    a.execute(
        "clothing.create",
        json!({"member_id":"default","name":"fresh top","category":"top"}),
    )
    .unwrap();
    a.execute("wear.record",json!({"member_id":"default","operation_key":"wear","local_date":chrono::Utc::now().date_naive().to_string(),"timezone":"UTC","clothing_ids":stock.iter().map(|c|c["id"].clone()).collect::<Vec<_>>()})).unwrap();
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
    let p=a.execute("provider.save",json!({"name":"repeat protocol","endpoint":endpoint,"model":"text","capabilities":["text"],"authentication":"none"})).unwrap();
    let r=a.execute("recommendation.generate",json!({"member_id":"default","operation_key":"recent-ai","timezone":"UTC","limit":1,"strategy":"ai","provider_id":p["id"]})).unwrap();
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
    let p=a.execute("provider.save",json!({"name":"valid protocol","endpoint":endpoint,"model":"text","capabilities":["text"],"authentication":"none"})).unwrap();
    let r=a.execute("recommendation.generate",json!({"member_id":"default","operation_key":"valid-ai","timezone":"UTC","limit":1,"strategy":"ai","provider_id":p["id"],"fixed_ids":[stock[0]["id"]]})).unwrap();
    worker.join().unwrap();
    assert_eq!(r["generation"]["source"], "text_model");
    assert_eq!(r["generation"]["attempts"], 1);
    assert_eq!(
        a.execute("recommendation.generate", r["controls"].clone())
            .unwrap(),
        r
    );
    a.execute("recommendation.save",json!({"member_id":"default","id":r["id"],"proposal_index":0,"name":"actual validated","operation_key":"save"})).unwrap();
    assert_eq!(
        a.execute("wear.list", json!({"member_id":"default"}))
            .unwrap()["items"],
        json!([])
    );
    assert_eq!(
        a.execute("outfit.list", json!({"member_id":"default"}))
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
        .execute(
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
                    .execute("clothing.archive", json!({"member_id":"default","id":old}))
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
    let p=a.execute("provider.save",json!({"name":"late protocol","endpoint":endpoint,"model":"text","capabilities":["text"],"authentication":"none"})).unwrap();
    let r=a.execute("recommendation.generate",json!({"member_id":"default","operation_key":"late-ai","timezone":"UTC","limit":1,"strategy":"ai","provider_id":p["id"]})).unwrap();
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
    let p=a.execute("provider.save",json!({"name":"race protocol","endpoint":endpoint,"model":"text","capabilities":["text"],"authentication":"none"})).unwrap();
    let input = json!({"member_id":"default","operation_key":"same-race","timezone":"UTC","limit":1,"strategy":"ai","provider_id":p["id"]});
    drop(a);
    let first_path = path.clone();
    let first_input = input.clone();
    let first = std::thread::spawn(move || {
        Application::open(&first_path)
            .unwrap()
            .execute("recommendation.generate", first_input)
    });
    received
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap();
    let second = Application::open(&path)
        .unwrap()
        .execute("recommendation.generate", input);
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
        let p=a.execute("provider.save",json!({"name":"lease protocol","endpoint":endpoint,"model":"text","capabilities":["text"],"authentication":"none"})).unwrap();
        let input = json!({"member_id":"default","operation_key":"lease","timezone":"UTC","limit":1,"strategy":"ai","provider_id":p["id"]});
        let request = input.clone();
        let worker_path = path.clone();
        let generation = std::thread::spawn(move || {
            Application::open(&worker_path)
                .unwrap()
                .execute("recommendation.generate", request)
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
            a.execute("jobs.cancel", args.clone()).unwrap();
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
            a.execute("jobs.get", args.clone()).unwrap()["submission_count"],
            1,
            "cancellation of a confirmed invalid first response must not grant a second submission"
        );
        assert_eq!(
            a.execute("jobs.retry", args.clone()).unwrap_err().code,
            "submission_already_sent"
        );
        assert_eq!(
            a.execute("recommendation.generate", input)
                .unwrap_err()
                .code,
            "generation_waiting_input"
        );
        if !cancel {
            assert_eq!(
                a.execute("jobs.get", args).unwrap()["state"],
                "waiting_input"
            );
        }
    }
}
