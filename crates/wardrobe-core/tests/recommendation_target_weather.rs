use serde_json::{json, Value};
use wardrobe_core::Application;

// Bounded public regression: a recommendation whose target_date is a future local day must
// not present the saved current-observation cache as target-day automatic weather. The
// date-aware resolver fix belongs to root; this file only pins the public contract.
// Controlled localhost weather fixture, same pattern as tests/weather.rs: exactly one
// Open-Meteo style current observation (21.5°C); the listener drops afterwards, so any
// further refresh would fail and surface through weather.get last_error.
fn current_observation_fixture() -> (String, std::thread::JoinHandle<()>) {
    let server = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}/v1/forecast", server.local_addr().unwrap());
    let worker = std::thread::spawn(move || {
        use std::io::{Read, Write};
        let (mut s, _) = server.accept().unwrap();
        let mut b = [0; 8192];
        let n = s.read(&mut b).unwrap();
        let req = String::from_utf8_lossy(&b[..n]);
        assert!(
            req.contains("latitude=31.23")
                && req.contains("longitude=121.47")
                && req.contains("temperature_2m")
        );
        let body=json!({"current_units":{"time":"unixtime","temperature_2m":"°C"},"current":{"time":chrono::Utc::now().timestamp(),"temperature_2m":21.5}}).to_string();
        write!(s,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).unwrap();
    });
    (endpoint, worker)
}

fn configure_and_refresh_known_cache(
    a: &Application,
    endpoint: &str,
    worker: std::thread::JoinHandle<()>,
) {
    a.execute("weather.configure",json!({"member_id":"default","mode":"automatic","location":"上海（公开示例）","latitude":31.23,"longitude":121.47,"endpoint":endpoint,"usage":"noncommercial"})).unwrap();
    let refreshed = a
        .execute("weather.refresh", json!({"member_id":"default"}))
        .unwrap();
    worker.join().unwrap();
    assert_eq!(refreshed["cache"]["temperature_c"], json!(21.5));
    assert_eq!(refreshed["cache"]["source"], json!("open_meteo_model"));
    assert_eq!(refreshed["cache"]["stale"], json!(false));
}

fn complete_wardrobe(a: &Application) {
    for (name, category) in [
        ("上衣A", "top"),
        ("长裤A", "bottom"),
        ("鞋A", "shoes"),
        ("外套A", "outerwear"),
    ] {
        a.execute(
            "clothing.create",
            json!({"member_id":"default","name":name,"category":category}),
        )
        .unwrap();
    }
}

#[test]
fn tomorrow_rules_recommendation_does_not_present_current_observation_as_target_day_weather() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("db");
    let a = Application::open(&path).unwrap();
    complete_wardrobe(&a);
    let (endpoint, worker) = current_observation_fixture();
    configure_and_refresh_known_cache(&a, &endpoint, worker);
    // Tomorrow derived from chrono now in the chosen IANA zone; no sleeps.
    let zone: chrono_tz::Tz = "Asia/Shanghai".parse().unwrap();
    let tomorrow = chrono::Utc::now()
        .with_timezone(&zone)
        .date_naive()
        .succ_opt()
        .unwrap()
        .to_string();
    let r = a.execute("recommendation.generate",json!({"member_id":"default","operation_key":"tomorrow-automatic","timezone":"Asia/Shanghai","strategy":"rules","target_date":tomorrow,"limit":1,"weather":{"mode":"automatic"}})).unwrap();
    let context = &r["weather_context"];
    assert_eq!(context["mode"], json!("automatic"));
    // Red oracle: the current-observation cache must not be presented as target-day weather.
    assert_eq!(context["source"], json!("unavailable"), "tomorrow target must not present the current observation as target-day weather; got {context}");
    assert!(
        context["temperature_c"].is_null(),
        "tomorrow target weather_context must not carry a temperature; got {context}"
    );
    assert_eq!(
        context["network_requested"],
        json!(false),
        "future target weather must stay offline"
    );
    assert_eq!(
        context["target_date"],
        json!(tomorrow),
        "weather_context must explicitly name the target date it applies to"
    );
    // The recommendation still succeeds offline and respects the explicit target date.
    assert_eq!(r["recent_window"]["through"], json!(tomorrow));
    assert_eq!(r["recent_window"]["timezone"], json!("Asia/Shanghai"));
    assert_eq!(r["actual_count"], json!(1));
    assert_eq!(r["proposals"][0]["complete"], json!(true));
    for role in ["top", "bottom", "shoes"] {
        assert!(r["proposals"][0]["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["role"] == role));
    }
    drop(a);
    // Success persists, and the saved current-observation cache stays untouched.
    let a = Application::open(&path).unwrap();
    assert_eq!(
        a.execute(
            "recommendation.get",
            json!({"member_id":"default","id":r["id"]})
        )
        .unwrap(),
        r
    );
    let weather = a
        .execute("weather.get", json!({"member_id":"default"}))
        .unwrap();
    assert_eq!(weather["cache"]["temperature_c"], json!(21.5));
    assert_eq!(weather["cache"]["source"], json!("open_meteo_model"));
    assert_eq!(
        weather["last_error"],
        Value::Null,
        "no refresh may be attempted for a future target date"
    );
}

// Control: same-day automatic weather keeps using the saved current observation.
#[test]
fn same_day_automatic_weather_remains_usable_with_explicit_target_date() {
    let d = tempfile::tempdir().unwrap();
    let a = Application::open(&d.path().join("db")).unwrap();
    complete_wardrobe(&a);
    let (endpoint, worker) = current_observation_fixture();
    configure_and_refresh_known_cache(&a, &endpoint, worker);
    let zone: chrono_tz::Tz = "Asia/Shanghai".parse().unwrap();
    let today = chrono::Utc::now()
        .with_timezone(&zone)
        .date_naive()
        .to_string();
    let r = a.execute("recommendation.generate",json!({"member_id":"default","operation_key":"today-automatic","timezone":"Asia/Shanghai","strategy":"rules","target_date":today,"limit":1,"weather":{"mode":"automatic"}})).unwrap();
    let context = &r["weather_context"];
    assert_eq!(context["mode"], json!("automatic"));
    assert_eq!(
        context["source"],
        json!("open_meteo_model"),
        "same-day automatic weather keeps the saved current observation"
    );
    assert_eq!(context["temperature_c"], json!(21.5));
    assert_eq!(
        context["network_requested"],
        json!(false),
        "a fresh cache needs no network for the same-day target"
    );
    assert_eq!(r["recent_window"]["through"], json!(today));
    assert_eq!(r["actual_count"], json!(1));
    assert_eq!(r["proposals"][0]["complete"], json!(true));
    // 21.5°C > 15: the observation actually drives planning — no cold outerwear requirement.
    assert!(r["proposals"][0]["items"]
        .as_array()
        .unwrap()
        .iter()
        .all(|i| i["role"] != "outerwear"));
}

// The real async AI worker path: enqueued with a future target_date and automatic weather,
// the member-scoped worker must also resolve the target day to explicit unavailability, and
// the model prompt must carry that context. The localhost provider fixture only pins this
// request contract; the served echo outfit uses real inventory IDs, and nothing here asserts
// model quality.
#[test]
fn ai_worker_prompt_carries_unavailable_target_day_context_for_future_date() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("db");
    let a = Application::open(&path).unwrap();
    complete_wardrobe(&a);
    let (weather_endpoint, weather_worker) = current_observation_fixture();
    configure_and_refresh_known_cache(&a, &weather_endpoint, weather_worker);
    let zone: chrono_tz::Tz = "Asia/Shanghai".parse().unwrap();
    let tomorrow = chrono::Utc::now()
        .with_timezone(&zone)
        .date_naive()
        .succ_opt()
        .unwrap()
        .to_string();
    let stock = a
        .execute("clothing.list", json!({"member_id":"default"}))
        .unwrap();
    let items: Vec<Value> = stock["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| ["top", "bottom", "shoes"].contains(&c["category"].as_str().unwrap()))
        .map(|c| json!({"clothing_id":c["id"],"role":c["category"],"layer":0}))
        .collect();
    let server = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}/v1", server.local_addr().unwrap());
    let prompt_target = tomorrow.clone();
    let worker = std::thread::spawn(move || {
        use std::io::{Read, Write};
        let (mut s, _) = server.accept().unwrap();
        let mut b = [0; 65536];
        let n = s.read(&mut b).unwrap();
        let req = String::from_utf8_lossy(&b[..n]);
        assert!(req.contains("/chat/completions"));
        // The prompt embeds the prompt JSON as an escaped string, hence the escaped quotes.
        assert!(
            req.contains("\\\"source\\\":\\\"unavailable\\\""),
            "worker prompt must carry the unavailable target-day weather"
        );
        assert!(
            req.contains("target_date_not_supported"),
            "worker prompt must explain why weather is unavailable"
        );
        assert!(
            req.contains(&prompt_target),
            "worker prompt must carry the explicit target date"
        );
        let body=json!({"choices":[{"message":{"content":json!({"proposals":[{"items":items}]}).to_string()}}]}).to_string();
        write!(
            s,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        )
        .unwrap();
    });
    let p=a.execute("provider.save",json!({"name":"protocol only","endpoint":endpoint,"model":"configured-text","capabilities":["text"],"authentication":"none"})).unwrap();
    let receipt = a.execute("recommendation.generate",json!({"member_id":"default","operation_key":"tomorrow-ai","timezone":"Asia/Shanghai","strategy":"ai","provider_id":p["id"],"limit":1,"target_date":tomorrow,"weather":{"mode":"automatic"}})).unwrap();
    assert!(
        receipt["job_id"].is_string(),
        "ai enqueue stays async before any weather or provider work"
    );
    let job = a
        .execute(
            "recommendation.worker.run",
            json!({"member_id":"default","id":receipt["job_id"]}),
        )
        .unwrap();
    worker.join().unwrap();
    let r = a
        .execute(
            "recommendation.get",
            json!({"member_id":"default","id":job["result"]["intent_id"]}),
        )
        .unwrap();
    let context = &r["weather_context"];
    assert_eq!(context["mode"], json!("automatic"));
    assert_eq!(
        context["source"],
        json!("unavailable"),
        "the async worker must not present the current observation as target-day weather; got {context}"
    );
    assert!(
        context["temperature_c"].is_null(),
        "the async worker weather_context must not carry a temperature; got {context}"
    );
    assert_eq!(
        context["network_requested"],
        json!(false),
        "future target weather must stay offline on the ai path too"
    );
    assert_eq!(
        context["target_date"],
        json!(tomorrow),
        "the ai intent must explicitly name the target date its weather applies to"
    );
    // The served echo outfit validated against real inventory — request contract only.
    assert_eq!(r["generation"]["source"], json!("text_model"));
    // The saved current-observation cache stays untouched by the ai path.
    let weather = a
        .execute("weather.get", json!({"member_id":"default"}))
        .unwrap();
    assert_eq!(weather["cache"]["temperature_c"], json!(21.5));
    assert_eq!(weather["cache"]["source"], json!("open_meteo_model"));
    assert_eq!(
        weather["last_error"],
        Value::Null,
        "no refresh may be attempted for a future target date on the ai path"
    );
}
