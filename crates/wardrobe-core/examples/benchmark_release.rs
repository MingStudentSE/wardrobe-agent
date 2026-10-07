//! #35 release performance baseline: 10000 items + 100000 effective wear events.
//!
//! Metrics (parent spec #1 Testing Decisions):
//! - cold start to first screen query (Application::open + clothing.list) x N
//! - local search/filter first screen p95 <= 500ms
//! - plain entity save p95 <= 300ms (no images, no network)
//! - explicit wear-event record (additional evidence)
//!
//! Seeding follows the accepted #15/#18 scale pattern: one real item and one
//! real wear event through the public Application API, then bulk copies of the
//! same document shapes. Synthetic data only; the caller supplies a dedicated
//! database path that must not exist.
use serde_json::json;
use std::time::Instant;
use wardrobe_core::Application;

fn stats(values: &[f64]) -> serde_json::Value {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let n = sorted.len();
    let mean = sorted.iter().sum::<f64>() / n as f64;
    json!({
        "samples": n,
        "min_ms": sorted[0],
        "p50_ms": sorted[(n * 50).div_ceil(100) - 1],
        "p95_ms": sorted[(n * 95).div_ceil(100) - 1],
        "max_ms": sorted[n - 1],
        "mean_ms": mean,
    })
}

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("provide a dedicated benchmark database path");
    let samples: usize = args
        .next()
        .and_then(|v| v.parse().ok())
        .unwrap_or(30);
    assert!(
        !std::path::Path::new(&path).exists(),
        "benchmark must use a new database"
    );
    let seeding = Instant::now();
    let app = Application::open(std::path::Path::new(&path)).unwrap();
    let member = app.execute("space.default", json!({})).unwrap()["member_id"]
        .clone();
    // One real seed document per entity kind through the public API.
    let seed = app
        .execute(
            "clothing.create",
            json!({"member_id":member,"name":"基准衬衫 0000","category":"top","color":["white"],"seasons":["spring"],"tags":["通勤"]}),
        )
        .unwrap();
    let wear = app
        .execute(
            "wear.record",
            json!({"member_id":member,"operation_key":"bench-seed","local_date":"2020-01-01","timezone":"UTC","clothing_ids":[seed["id"]]}),
        )
        .unwrap();
    drop(app);
    // Bulk copies of the same accepted document shapes (see recommendations.rs
    // scale_ten_thousand_inventory_hundred_thousand_effective_wears).
    let mut db = rusqlite::Connection::open(&path).unwrap();
    let tx = db.transaction().unwrap();
    {
        let mut clothing = tx
            .prepare("INSERT INTO clothing VALUES(?1,?2,?3)")
            .unwrap();
        for i in 0..9999 {
            let mut c = seed.clone();
            c["id"] = json!(format!("bench-clothing-{i:05}"));
            c["name"] = json!(format!("基准衬衫 {i:04}"));
            c["category"] = json!(["top", "bottom", "shoes"][i % 3]);
            clothing
                .execute(rusqlite::params![
                    c["id"].as_str(),
                    c["member_id"].as_str(),
                    c.to_string()
                ])
                .unwrap();
        }
        let mut events = tx
            .prepare("INSERT INTO wear_events VALUES(?1,?2,?1,1,?3)")
            .unwrap();
        for i in 0..99999 {
            let mut w = wear.clone();
            let id = format!("bench-wear-{i:06}");
            w["id"] = json!(id.clone());
            w["root_event_id"] = json!(id);
            events
                .execute(rusqlite::params![
                    w["id"].as_str(),
                    w["member_id"].as_str(),
                    w.to_string()
                ])
                .unwrap();
        }
    }
    tx.commit().unwrap();
    drop(db);
    let seeding_ms = seeding.elapsed().as_secs_f64() * 1000.0;

    // Cold start to first screen: open + first local list query.
    let mut cold = vec![];
    for _ in 0..samples {
        let started = Instant::now();
        let app = Application::open(std::path::Path::new(&path)).unwrap();
        let page = app
            .execute(
                "clothing.list",
                json!({"member_id":member,"limit":24}),
            )
            .unwrap();
        assert_eq!(page["items"].as_array().unwrap().len(), 24);
        cold.push(started.elapsed().as_secs_f64() * 1000.0);
        drop(app);
    }

    let app = Application::open(std::path::Path::new(&path)).unwrap();
    // Local search/filter first screen.
    let mut search = vec![];
    for _ in 0..samples {
        let started = Instant::now();
        let page = app
            .execute(
                "clothing.list",
                json!({"member_id":member,"search":"衬衫","color":"white","season":"spring","tag":"通勤","sort":"name_asc","limit":24}),
            )
            .unwrap();
        assert_eq!(page["items"].as_array().unwrap().len(), 24);
        search.push(started.elapsed().as_secs_f64() * 1000.0);
    }
    // Plain entity save (no images, no network).
    let mut save = vec![];
    let first = app
        .execute(
            "clothing.list",
            json!({"member_id":member,"search":"衬衫","limit":1}),
        )
        .unwrap()["items"][0]["id"]
        .clone();
    for i in 0..samples {
        let started = Instant::now();
        app.execute(
            "clothing.update",
            json!({"member_id":member,"id":first,"brand":format!("基线 {i}")}),
        )
        .unwrap();
        save.push(started.elapsed().as_secs_f64() * 1000.0);
    }
    // Explicit wear record (additional evidence beyond the plain save gate).
    let mut wear_save = vec![];
    for i in 0..samples {
        let started = Instant::now();
        app.execute(
            "wear.record",
            json!({"member_id":member,"operation_key":format!("bench-wear-run-{i}"),"local_date":"2026-10-07","timezone":"Asia/Shanghai","clothing_ids":[first]}),
        )
        .unwrap();
        wear_save.push(started.elapsed().as_secs_f64() * 1000.0);
    }
    let db_bytes = std::fs::metadata(&path).unwrap().len();
    drop(app);
    println!(
        "{}",
        json!({
            "scope":"release performance baseline on dedicated synthetic database; GUI cold start measured separately",
            "inventory":10000,
            "effective_wears":100000,
            "samples":samples,
            "seeding_total_ms":seeding_ms,
            "db_bytes":db_bytes,
            "cold_start_to_first_screen":stats(&cold),
            "cold_start_samples_ms":cold,
            "search_filter_first_screen":stats(&search),
            "search_samples_ms":search,
            "plain_entity_save":stats(&save),
            "save_samples_ms":save,
            "wear_event_record":stats(&wear_save),
            "wear_samples_ms":wear_save,
            "os":std::env::consts::OS,
            "arch":std::env::consts::ARCH,
        })
    );
}
