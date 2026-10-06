use serde_json::{json, Value};
use wardrobe_core::Application;
#[test]
fn configured_weather_refresh_persists_real_http_result_and_failure_keeps_manual_off_available() {
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
        let body=json!({"current_units":{"time":"unixtime","temperature_2m":"°C"},"current":{"time":1791324000,"temperature_2m":21.5}}).to_string();
        write!(s,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).unwrap();
    });
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("db");
    let a = Application::open(&p).unwrap();
    let config = json!({"member_id":"default","mode":"automatic","location":"上海（公开示例）","latitude":31.23,"longitude":121.47,"endpoint":endpoint,"usage":"noncommercial"});
    a.execute("weather.configure", config).unwrap();
    let r = a
        .execute("weather.refresh", json!({"member_id":"default"}))
        .unwrap();
    worker.join().unwrap();
    assert_eq!(r["cache"]["temperature_c"], 21.5);
    assert_eq!(r["cache"]["stale"], false);
    assert!(r["cache"]["obtained_at"].is_string());
    assert_eq!(r["cache"]["source"], "open_meteo_model");
    drop(a);
    let a = Application::open(&p).unwrap();
    assert_eq!(
        a.execute("weather.get", json!({"member_id":"default"}))
            .unwrap()["cache"]["temperature_c"],
        21.5
    );
    let failed = a
        .execute("weather.refresh", json!({"member_id":"default"}))
        .unwrap();
    assert_eq!(failed["cache"]["temperature_c"], 21.5);
    assert!(failed["last_error"].is_object());
    a.execute(
        "weather.configure",
        json!({"member_id":"default","mode":"off"}),
    )
    .unwrap();
    assert_eq!(
        a.execute("weather.refresh", json!({"member_id":"default"}))
            .unwrap()["config"]["mode"],
        "off"
    );
    let r=a.execute("recommendation.generate",json!({"member_id":"default","operation_key":"manual","timezone":"Asia/Shanghai","occasion":"work","weather":{"mode":"manual","temperature_c":20}})).unwrap();
    assert_eq!(r["weather_context"]["source"], "manual");
}
#[test]
fn weather_cache_is_fresh_before_three_hours_and_stale_after_and_highest_version_repairs_table() {
    let server = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}/v1/forecast", server.local_addr().unwrap());
    let w = std::thread::spawn(move || {
        use std::io::{Read, Write};
        let (mut s, _) = server.accept().unwrap();
        let mut b = [0; 8192];
        assert!(s.read(&mut b).unwrap() > 0);
        let body = r#"{"current_units":{"time":"unixtime","temperature_2m":"°C"},"current":{"time":1791324000,"temperature_2m":21.5}}"#;
        write!(
            s,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        )
        .unwrap();
    });
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("db");
    let a = Application::open(&path).unwrap();
    let c = a
        .execute(
            "clothing.create",
            json!({"member_id":"default","name":"retained fact","category":"top"}),
        )
        .unwrap();
    a.execute("weather.configure",json!({"member_id":"default","mode":"automatic","location":"public city","latitude":31.23,"longitude":121.47,"endpoint":endpoint,"usage":"noncommercial"})).unwrap();
    let doc = a
        .execute("weather.refresh", json!({"member_id":"default"}))
        .unwrap();
    w.join().unwrap();
    drop(a);
    // Timestamp fixture only; observations are through public weather.get.
    for (age, expected) in [(10795, false), (10805, true)] {
        let mut fixture = doc.clone();
        fixture["cache"]["obtained_at"] =
            json!((chrono::Utc::now() - chrono::Duration::seconds(age)).to_rfc3339());
        let db = rusqlite::Connection::open(&path).unwrap();
        db.execute(
            "UPDATE weather_preferences SET document=?1",
            [fixture.to_string()],
        )
        .unwrap();
        drop(db);
        let a = Application::open(&path).unwrap();
        assert_eq!(
            a.execute("weather.get", json!({"member_id":"default"}))
                .unwrap()["cache"]["stale"],
            expected
        );
    }
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("DROP TABLE weather_preferences; PRAGMA user_version=18;")
        .unwrap();
    drop(db);
    let a = Application::open(&path).unwrap();
    assert_eq!(
        a.execute("weather.get", json!({"member_id":"default"}))
            .unwrap()["config"]["mode"],
        "off"
    );
    assert_eq!(
        a.execute("clothing.get", json!({"member_id":"default","id":c["id"]}))
            .unwrap(),
        c
    );
}
#[test]
fn late_weather_response_cannot_restore_location_after_user_disables_weather() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("db");
    let a = Application::open(&path).unwrap();
    let server = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}/v1/forecast", server.local_addr().unwrap());
    a.execute("weather.configure",json!({"member_id":"default","mode":"automatic","location":"public city","latitude":31.23,"longitude":121.47,"endpoint":endpoint,"usage":"noncommercial"})).unwrap();
    let w = std::thread::spawn(move || {
        use std::io::{Read, Write};
        let (mut s, _) = server.accept().unwrap();
        let mut b = [0; 8192];
        assert!(s.read(&mut b).unwrap() > 0);
        let other = Application::open(&path).unwrap();
        other
            .execute(
                "weather.configure",
                json!({"member_id":"default","mode":"off"}),
            )
            .unwrap();
        let body = r#"{"current_units":{"time":"unixtime","temperature_2m":"°C"},"current":{"time":1791324000,"temperature_2m":21.5}}"#;
        write!(
            s,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        )
        .unwrap();
    });
    let r = a
        .execute("weather.refresh", json!({"member_id":"default"}))
        .unwrap();
    w.join().unwrap();
    assert_eq!(r["config"]["mode"], "off");
    assert_eq!(r["cache"], Value::Null);
}
