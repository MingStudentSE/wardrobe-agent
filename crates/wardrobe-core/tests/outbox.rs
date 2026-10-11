//! #71 outbox 停写契约：业务写路径不再向 outbox 遗留表写入任何行。
//!
//! outbox 是历史迁移遗留的死表（全库只有写者、零读路径）：真实同步发布链走
//! sync_journal 触发器 + sync_publications + commit markers（sync/index.rs），
//! 从不消费 outbox。表结构与既有数据按「schema 迁移只向前」保留，因此本测试
//! 直接以该表为观察点断言零行；同步行为本身由 sync_* 契约测试守护。
use serde_json::json;
use wardrobe_core::Application;

#[path = "support/learning.rs"]
mod learning_support;
use learning_support::LearningTestSeam;

fn outbox_rows(database: &std::path::Path) -> i64 {
    let connection = rusqlite::Connection::open(database).unwrap();
    connection
        .query_row("SELECT count(*) FROM outbox", [], |r| r.get(0))
        .unwrap()
}

#[test]
fn business_write_paths_leave_the_legacy_outbox_table_empty() {
    let dir = tempfile::tempdir().unwrap();
    let database = dir.path().join("wardrobe.sqlite");
    let app = Application::open(&database).unwrap();
    let source = dir.path().join("shirt.png");
    image::RgbImage::from_pixel(48, 24, image::Rgb([247, 246, 242]))
        .save(&source)
        .unwrap();

    // 单品：创建、修订（clothing.rs 两处写点）。
    let garment = app
        .execute_and_wait(
            "clothing.create",
            json!({"member_id":"default","name":"白衬衫","category":"top","color":["white"]}),
        )
        .unwrap();
    app.execute_and_wait(
        "clothing.update",
        json!({"member_id":"default","id":garment["id"],"name":"白衬衫改名"}),
    )
    .unwrap();

    // 自定义类目：创建、删除（categories.rs）。
    let category = app
        .execute_and_wait(
            "category.create",
            json!({"member_id":"default","name":"汉服"}),
        )
        .unwrap();
    app.execute_and_wait(
        "category.delete",
        json!({"member_id":"default","id":category["id"]}),
    )
    .unwrap();

    // 资产：导入、领用、编辑与领用链接（assets.rs 三处、image_editing.rs）。
    let asset = app
        .execute_and_wait(
            "assets.import",
            json!({"member_id":"default","path":source}),
        )
        .unwrap();
    app.execute_and_wait(
        "assets.adopt",
        json!({"member_id":"default","asset_id":asset["id"],"clothing_id":garment["id"]}),
    )
    .unwrap();
    app.execute_and_wait(
        "images.rotate",
        json!({"member_id":"default","asset_id":asset["id"],"degrees":90}),
    )
    .unwrap();

    // 搭配（outfits.rs）。
    app.execute_and_wait(
        "outfit.create",
        json!({"member_id":"default","name":"周末","items":[{"clothing_id":garment["id"],"role":"base","layer":0}]}),
    )
    .unwrap();

    // 穿着（wears.rs）与护理（care.rs）。
    app.execute_and_wait(
        "wear.record",
        json!({"member_id":"default","operation_key":"w1","local_date":"2020-01-01","timezone":"UTC","clothing_ids":[garment["id"]]}),
    )
    .unwrap();
    app.execute_and_wait(
        "care.record",
        json!({"member_id":"default","clothing_id":garment["id"],"action":"wash","operation_key":"wash1","local_date":"2020-01-02","timezone":"UTC"}),
    )
    .unwrap();

    // 见解偏好（insights.rs）与学习画像（learning.rs 三处）。
    app.execute_and_wait(
        "insights.configure",
        json!({"member_id":"default","stale_days":30,"expected_revision":0}),
    )
    .unwrap();
    let explicit = app
        .execute_and_wait(
            "learning.set",
            json!({"member_id":"default","operation_key":"prefer","dimension":"style","value":"casual","weight":4}),
        )
        .unwrap();
    app.execute_and_wait(
        "learning.edit",
        json!({"member_id":"default","operation_key":"edit","root_id":explicit["id"],"expected_revision":1,"weight":2,"note":"降低偏好"}),
    )
    .unwrap();
    app.execute_and_wait(
        "learning.recompute",
        json!({"member_id":"default","operation_key":"learn"}),
    )
    .unwrap();

    // 推荐（读路径，不持久化）、反馈（feedback.rs 两处写点之一）。
    let intent = app
        .execute_and_wait(
            "recommendation.generate",
            json!({"member_id":"default","operation_key":"r1","timezone":"UTC"}),
        )
        .unwrap();
    app.execute_and_wait(
        "feedback.record",
        json!({"member_id":"default","operation_key":"f1","intent_id":intent["id"],"proposal_index":0,"reaction":"like"}),
    )
    .unwrap();

    // 日程（schedules.rs）。
    let device = app.execute_and_wait("schedule.device", json!({})).unwrap();
    app.execute_and_wait(
        "schedule.create",
        json!({"member_id":"default","name":"护理提醒","kind":"care","local_time":"08:00","timezone":"UTC","weekdays":[1,2],"execution_device_id":device["id"],"occasion":"daily","target_day_offset":0,"enabled":true}),
    )
    .unwrap();

    assert_eq!(
        outbox_rows(&database),
        0,
        "业务写路径不得再向 outbox 遗留表写入"
    );
}
