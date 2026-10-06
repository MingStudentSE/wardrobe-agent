//! Reproducible inventory baseline; final Beta also requires wear-event and GUI cold-start gates.
use serde_json::json;
use std::time::Instant;
use wardrobe_core::Application;
fn p95(mut values: Vec<f64>) -> f64 {
    values.sort_by(f64::total_cmp);
    values[(values.len() * 95).div_ceil(100) - 1]
}
fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("provide a dedicated benchmark database path");
    assert!(
        !std::path::Path::new(&path).exists(),
        "benchmark must use a new database"
    );
    let app = Application::open(std::path::Path::new(&path)).unwrap();
    let member = app.execute("space.default", json!({})).unwrap()["member_id"].clone();
    for i in 0..10000 {
        app.execute("clothing.create", json!({"member_id":member,"name":format!("通勤衬衫 {i}"),"category":"top","color":["white"],"seasons":["spring"],"tags":["通勤"]})).unwrap();
    }
    let mut search = vec![];
    let mut save = vec![];
    for i in 0..30 {
        let started = Instant::now();
        let page = app.execute("clothing.list", json!({"member_id":member,"search":"衬衫","color":"white","season":"spring","tag":"通勤","sort":"name_asc","limit":24})).unwrap();
        search.push(started.elapsed().as_secs_f64() * 1000.0);
        assert_eq!(page["items"].as_array().unwrap().len(), 24);
        let started = Instant::now();
        app.execute(
            "clothing.update",
            json!({"member_id":member,"id":page["items"][0]["id"],"brand":format!("基线 {i}")}),
        )
        .unwrap();
        save.push(started.elapsed().as_secs_f64() * 1000.0);
    }
    println!(
        "{}",
        json!({"scope":"inventory-only baseline; not final Beta acceptance","inventory":10000,"samples":30,"search_p95_ms":p95(search),"save_p95_ms":p95(save),"os":std::env::consts::OS,"arch":std::env::consts::ARCH})
    );
}
