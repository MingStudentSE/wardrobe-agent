use serde_json::{json, Value};
use wardrobe_core::{operations, Application};
#[test]
fn explicit_feedback_persists_idempotently_with_optional_scores_without_wear() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("db");
    let a = Application::open(&path).unwrap();
    for (name, category) in [("衬衫", "top"), ("长裤", "bottom"), ("鞋", "shoes")] {
        a.execute(
            "clothing.create",
            json!({"member_id":"default","name":name,"category":category}),
        )
        .unwrap();
    }
    let intent = a
        .execute(
            "recommendation.generate",
            json!({"member_id":"default","operation_key":"r","timezone":"UTC"}),
        )
        .unwrap();
    let input = json!({"member_id":"default","operation_key":"f","intent_id":intent["id"],"proposal_index":0,"reaction":"like","scores":{"overall":5,"comfort":null,"style":4}});
    let f = a.execute("feedback.record", input.clone()).unwrap();
    assert_eq!(a.execute("feedback.record", input).unwrap(), f);
    assert_eq!(f["scores"]["comfort"], json!(null));
    assert_eq!(
        a.execute("wear.list", json!({"member_id":"default"}))
            .unwrap()["items"],
        json!([])
    );
    drop(a);
    let a = Application::open(&path).unwrap();
    assert_eq!(
        a.execute("feedback.get", json!({"member_id":"default","id":f["id"]}))
            .unwrap(),
        f
    );
    assert_eq!(
        a.execute("feedback.list", json!({"member_id":"default"}))
            .unwrap()["items"],
        json!([f])
    );
}
#[test]
fn day_exclusion_is_explicit_local_day_hard_constraint_and_never_wear() {
    let d = tempfile::tempdir().unwrap();
    let a = Application::open(&d.path().join("db")).unwrap();
    let top = a
        .execute(
            "clothing.create",
            json!({"member_id":"default","name":"A","category":"top"}),
        )
        .unwrap();
    for (name, category) in [("B", "top"), ("裤", "bottom"), ("鞋", "shoes")] {
        a.execute(
            "clothing.create",
            json!({"member_id":"default","name":name,"category":category}),
        )
        .unwrap();
    }
    let r=a.execute("recommendation.generate",json!({"member_id":"default","operation_key":"r","timezone":"Pacific/Kiritimati","fixed_ids":[top["id"]]})).unwrap();
    let f=a.execute("feedback.record",json!({"member_id":"default","operation_key":"day","intent_id":r["id"],"proposal_index":0,"reaction":"day_exclude","timezone":"Pacific/Kiritimati","clothing_ids":[top["id"]]})).unwrap();
    assert_eq!(f["day_scope"]["timezone"], "Pacific/Kiritimati");
    assert!(f["day_scope"]["expires_at"].is_string());
    assert_eq!(a.execute("recommendation.generate",json!({"member_id":"default","operation_key":"conflict","timezone":"Pacific/Kiritimati","fixed_ids":[top["id"]]})).unwrap_err().code,"constraint_conflict");
    let next = a
        .execute(
            "recommendation.generate",
            json!({"member_id":"default","operation_key":"next","timezone":"Pacific/Kiritimati"}),
        )
        .unwrap();
    for p in next["proposals"].as_array().unwrap() {
        assert!(!p["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["clothing_id"] == top["id"]));
    }
    assert_eq!(
        a.execute("wear.list", json!({"member_id":"default"}))
            .unwrap()["items"],
        json!([])
    );
}
#[test]
fn actual_wear_requires_confirmation_and_changed_combination_is_a_real_idempotent_wear() {
    let d = tempfile::tempdir().unwrap();
    let a = Application::open(&d.path().join("db")).unwrap();
    let mut ids = vec![];
    for (name, category) in [
        ("A", "top"),
        ("B", "top"),
        ("裤", "bottom"),
        ("鞋", "shoes"),
    ] {
        ids.push(
            a.execute(
                "clothing.create",
                json!({"member_id":"default","name":name,"category":category}),
            )
            .unwrap()["id"]
                .clone(),
        );
    }
    let intent=a.execute("recommendation.generate",json!({"member_id":"default","operation_key":"r","timezone":"UTC","fixed_ids":[ids[0]]})).unwrap();
    let mut input = json!({"member_id":"default","operation_key":"actual","intent_id":intent["id"],"proposal_index":0,"reaction":"changed","wear":{"confirmed":false,"local_date":"2020-01-01","timezone":"UTC","clothing_ids":[ids[1],ids[2],ids[3]]}});
    assert_eq!(
        a.execute("feedback.record", input.clone())
            .unwrap_err()
            .code,
        "confirmation_required"
    );
    input["wear"]["confirmed"] = json!(true);
    let f = a.execute("feedback.record", input.clone()).unwrap();
    assert_eq!(a.execute("feedback.record", input).unwrap(), f);
    let list = a
        .execute("wear.list", json!({"member_id":"default"}))
        .unwrap();
    assert_eq!(list["items"].as_array().unwrap().len(), 1);
    assert_eq!(list["items"][0]["id"], f["wear_event_id"]);
    assert!(list["items"][0]["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v["clothing_id"] == ids[1]));
    assert!(!list["items"][0]["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v["clothing_id"] == ids[0]));
}
#[test]
fn corrected_revoked_feedback_keeps_readable_history_without_rewriting_proposal_or_wear() {
    let d = tempfile::tempdir().unwrap();
    let a = Application::open(&d.path().join("db")).unwrap();
    for (name, category) in [("A", "top"), ("裤", "bottom"), ("鞋", "shoes")] {
        a.execute(
            "clothing.create",
            json!({"member_id":"default","name":name,"category":category}),
        )
        .unwrap();
    }
    let r = a
        .execute(
            "recommendation.generate",
            json!({"member_id":"default","operation_key":"r","timezone":"UTC"}),
        )
        .unwrap();
    let f=a.execute("feedback.record",json!({"member_id":"default","operation_key":"f","intent_id":r["id"],"proposal_index":0,"reaction":"like"})).unwrap();
    let c=a.execute("feedback.correct",json!({"member_id":"default","operation_key":"c","root_id":f["root_id"],"expected_revision":1,"reaction":"dislike","scores":{"comfort":2}})).unwrap();
    assert_eq!(c["proposal"], f["proposal"]);
    assert_eq!(c["revision"], 2);
    assert_eq!(a.execute("feedback.correct",json!({"member_id":"default","operation_key":"stale","root_id":f["root_id"],"expected_revision":1,"reaction":"skip"})).unwrap_err().code,"version_conflict");
    let x=a.execute("feedback.revoke",json!({"member_id":"default","operation_key":"x","root_id":f["root_id"],"expected_revision":2,"reason":"误操作"})).unwrap();
    assert_eq!(x["revoked"], true);
    assert_eq!(
        a.execute("feedback.list", json!({"member_id":"default"}))
            .unwrap()["items"],
        json!([])
    );
    assert_eq!(
        a.execute(
            "feedback.history",
            json!({"member_id":"default","root_id":f["root_id"]})
        )
        .unwrap()["items"],
        json!([f, c, x])
    );
    assert_eq!(
        a.execute("wear.list", json!({"member_id":"default"}))
            .unwrap()["items"],
        json!([])
    );
}
#[test]
fn try_another_records_skip_preserves_fixed_and_disliked_sets_change_future_candidates() {
    let d = tempfile::tempdir().unwrap();
    let a = Application::open(&d.path().join("db")).unwrap();
    let fixed = a
        .execute(
            "clothing.create",
            json!({"member_id":"default","name":"鞋","category":"shoes"}),
        )
        .unwrap();
    for (name, category) in [("A", "top"), ("B", "top"), ("裤", "bottom")] {
        a.execute(
            "clothing.create",
            json!({"member_id":"default","name":name,"category":category}),
        )
        .unwrap();
    }
    let r=a.execute("recommendation.generate",json!({"member_id":"default","operation_key":"r","timezone":"UTC","fixed_ids":[fixed["id"]],"limit":1})).unwrap();
    let request = json!({"member_id":"default","operation_key":"another","intent_id":r["id"],"proposal_index":0});
    let next = a.execute("feedback.try_another", request.clone()).unwrap();
    assert_eq!(a.execute("feedback.try_another", request).unwrap(), next);
    let child = a
        .execute(
            "recommendation.get",
            json!({"member_id":"default","id":next["recommendation"]["id"]}),
        )
        .unwrap();
    assert_ne!(child["proposals"][0]["items"], r["proposals"][0]["items"]);
    assert!(child["proposals"][0]["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|i| i["clothing_id"] == fixed["id"]));
    assert_eq!(
        a.execute("feedback.list", json!({"member_id":"default"}))
            .unwrap()["items"][0]["reaction"],
        "skip"
    );
    a.execute("feedback.record",json!({"member_id":"default","operation_key":"dislike","intent_id":r["id"],"proposal_index":0,"reaction":"dislike"})).unwrap();
    let future=a.execute("recommendation.generate",json!({"member_id":"default","operation_key":"future","timezone":"UTC","limit":5,"fixed_ids":[fixed["id"]]})).unwrap();
    assert_eq!(future["actual_count"], 1);
    assert_eq!(
        future["proposals"][0]["items"],
        child["proposals"][0]["items"]
    );
    assert_eq!(
        a.execute("wear.list", json!({"member_id":"default"}))
            .unwrap()["items"],
        json!([])
    );
}
#[test]
fn day_exclusion_during_real_model_http_prevents_late_publication_of_fixed_garment() {
    use std::io::{Read, Write};
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("db");
    let a = Application::open(&path).unwrap();
    let mut stock = vec![];
    for (name, category) in [("A", "top"), ("裤", "bottom"), ("鞋", "shoes")] {
        stock.push(
            a.execute(
                "clothing.create",
                json!({"member_id":"default","name":name,"category":category}),
            )
            .unwrap(),
        );
    }
    let base=a.execute("recommendation.generate",json!({"member_id":"default","operation_key":"base","timezone":"UTC","fixed_ids":[stock[0]["id"]]})).unwrap();
    let server = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}/v1", server.local_addr().unwrap());
    let (seen, received) = std::sync::mpsc::channel();
    let (release, gate) = std::sync::mpsc::channel();
    let items: Vec<_> = stock
        .iter()
        .map(|c| json!({"clothing_id":c["id"],"role":c["category"],"layer":0}))
        .collect();
    let body=json!({"choices":[{"message":{"content":json!({"proposals":[{"items":items}]}).to_string()}}]}).to_string();
    let protocol = std::thread::spawn(move || {
        let (mut stream, _) = server.accept().unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(10)))
            .unwrap();
        let mut b = [0; 65536];
        assert!(stream.read(&mut b).unwrap() > 0);
        seen.send(()).unwrap();
        gate.recv_timeout(std::time::Duration::from_secs(10))
            .unwrap();
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        )
        .unwrap();
    });
    let provider=a.execute("provider.save",json!({"name":"late feedback protocol","endpoint":endpoint,"model":"text","capabilities":["text"],"authentication":"none"})).unwrap();
    let receipt=a.execute("recommendation.generate",json!({"member_id":"default","operation_key":"ai","timezone":"UTC","fixed_ids":[stock[0]["id"]],"strategy":"ai","provider_id":provider["id"],"limit":1})).unwrap();
    let wp = path.clone();
    let id = receipt["job_id"].clone();
    let worker = std::thread::spawn(move || {
        Application::open(&wp).unwrap().execute(
            "recommendation.worker.run",
            json!({"member_id":"default","id":id}),
        )
    });
    received
        .recv_timeout(std::time::Duration::from_secs(10))
        .unwrap();
    a.execute("feedback.record",json!({"member_id":"default","operation_key":"exclude","intent_id":base["id"],"proposal_index":0,"reaction":"day_exclude","timezone":"UTC","clothing_ids":[stock[0]["id"]]})).unwrap();
    release.send(()).unwrap();
    assert_eq!(
        worker.join().unwrap().unwrap()["error"]["code"],
        "constraint_conflict"
    );
    protocol.join().unwrap();
    assert_eq!(
        a.execute("recommendation.list", json!({"member_id":"default"}))
            .unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let job = a
        .execute(
            "jobs.get",
            json!({"member_id":"default","id":receipt["job_id"]}),
        )
        .unwrap();
    assert_eq!(job["state"], "failed");
    assert!(job["result"].is_null());
}
#[test]
fn feedback_retains_adopted_display_after_detach_and_delete_without_original_sync() {
    let d = tempfile::tempdir().unwrap();
    let a = Application::open(&d.path().join("db")).unwrap();
    let c = a
        .execute(
            "clothing.create",
            json!({"member_id":"default","name":"A","category":"top"}),
        )
        .unwrap();
    let image_path = d.path().join("public.png");
    image::RgbaImage::from_pixel(16, 16, image::Rgba([255, 0, 0, 255]))
        .save(&image_path)
        .unwrap();
    let asset = a
        .execute(
            "assets.import",
            json!({"member_id":"default","path":image_path}),
        )
        .unwrap();
    a.execute(
        "assets.adopt",
        json!({"member_id":"default","clothing_id":c["id"],"asset_id":asset["id"]}),
    )
    .unwrap();
    for (name, category) in [("裤", "bottom"), ("鞋", "shoes")] {
        a.execute(
            "clothing.create",
            json!({"member_id":"default","name":name,"category":category}),
        )
        .unwrap();
    }
    let r=a.execute("recommendation.generate",json!({"member_id":"default","operation_key":"r","timezone":"UTC","fixed_ids":[c["id"]]})).unwrap();
    let f=a.execute("feedback.record",json!({"member_id":"default","operation_key":"f","intent_id":r["id"],"proposal_index":0,"reaction":"like"})).unwrap();
    a.execute(
        "assets.detach",
        json!({"member_id":"default","clothing_id":c["id"],"asset_id":asset["id"]}),
    )
    .unwrap();
    assert_eq!(
        a.execute(
            "assets.get",
            json!({"member_id":"default","id":asset["id"]})
        )
        .unwrap()["state"],
        "adopted"
    );
    assert_eq!(
        a.execute("assets.sync_manifest", json!({"member_id":"default"}))
            .unwrap()["assets"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    a.execute(
        "clothing.archive",
        json!({"member_id":"default","id":c["id"]}),
    )
    .unwrap();
    a.execute(
        "clothing.delete",
        json!({"member_id":"default","id":c["id"]}),
    )
    .unwrap();
    assert!(f["inventory_snapshots"]
        .as_array()
        .unwrap()
        .iter()
        .any(|s| s["display_asset_id"] == asset["id"]));
    assert_eq!(
        a.execute("feedback.get", json!({"member_id":"default","id":f["id"]}))
            .unwrap(),
        f
    );
    assert!(a
        .execute(
            "assets.preview",
            json!({"member_id":"default","id":asset["id"]})
        )
        .is_ok());
}
#[test]
fn optional_score_validation_pagination_and_scoped_permissions_are_real_public_contracts() {
    let d = tempfile::tempdir().unwrap();
    let a = Application::open(&d.path().join("db")).unwrap();
    let member = a.execute("space.default", json!({})).unwrap()["member_id"].clone();
    for (name, category) in [("A", "top"), ("裤", "bottom"), ("鞋", "shoes")] {
        a.execute(
            "clothing.create",
            json!({"member_id":"default","name":name,"category":category}),
        )
        .unwrap();
    }
    let r = a
        .execute(
            "recommendation.generate",
            json!({"member_id":"default","operation_key":"r","timezone":"UTC"}),
        )
        .unwrap();
    for bad in [json!(0), json!(6), json!(2.5), json!("3"), json!(true)] {
        assert_eq!(a.execute("feedback.record",json!({"member_id":"default","operation_key":"bad","intent_id":r["id"],"proposal_index":0,"reaction":"like","scores":{"overall":bad}})).unwrap_err().code,"invalid_input");
    }
    let request = json!({"member_id":member,"operation_key":"f","intent_id":r["id"],"proposal_index":0,"reaction":"skip"});
    let f = a.execute("feedback.record", request.clone()).unwrap();
    let mut conflict = request;
    conflict["reaction"] = json!("dislike");
    assert_eq!(
        a.execute("feedback.record", conflict).unwrap_err().code,
        "idempotency_conflict"
    );
    for n in 0..3 {
        a.execute("feedback.record",json!({"member_id":member,"operation_key":format!("page{n}"),"intent_id":r["id"],"proposal_index":0,"reaction":"like"})).unwrap();
    }
    let first = a
        .execute("feedback.list", json!({"member_id":member,"limit":2}))
        .unwrap();
    let second = a
        .execute(
            "feedback.list",
            json!({"member_id":member,"limit":2,"cursor":first["next_cursor"]}),
        )
        .unwrap();
    assert_eq!(first["items"].as_array().unwrap().len(), 2);
    assert_eq!(second["items"].as_array().unwrap().len(), 2);
    assert!(second["next_cursor"].is_null());
    assert_ne!(first["items"][0]["id"], second["items"][0]["id"]);
    let other = a.execute("member.create", json!({"name":"other"})).unwrap();
    assert_eq!(
        a.execute(
            "feedback.get",
            json!({"member_id":other["member_id"],"id":f["id"]})
        )
        .unwrap_err()
        .code,
        "not_found"
    );
    let grant=a.execute("client.authorize",json!({"name":"feedback-only","expires_at":null,"mode":"limited","operations":["feedback.record","feedback.get","feedback.list","feedback.try_another"],"member_ids":[member]})).unwrap();
    let token = grant["token"].as_str().unwrap();
    assert_eq!(a.execute_as_client(token,"feedback.record",json!({"member_id":member,"operation_key":"wear-denied","intent_id":r["id"],"proposal_index":0,"reaction":"actual_wore","wear":{"confirmed":true,"local_date":"2020-01-01","timezone":"UTC"}})).unwrap_err().code,"scope_denied");
    assert_eq!(a.execute_as_client(token,"feedback.try_another",json!({"member_id":member,"operation_key":"try-denied","intent_id":r["id"],"proposal_index":0})).unwrap_err().code,"scope_denied");
    assert_eq!(
        a.execute_as_client(
            token,
            "feedback.get",
            json!({"member_id":other["member_id"],"id":f["id"]})
        )
        .unwrap_err()
        .code,
        "member_denied"
    );
    assert_eq!(
        a.execute_as_client(
            token,
            "feedback.get",
            json!({"member_id":member,"id":f["id"]})
        )
        .unwrap(),
        f
    );
}
#[test]
fn nineteen_repairs_missing_feedback_tables_and_two_clients_cannot_correct_one_revision() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("db");
    let a = Application::open(&path).unwrap();
    drop(a);
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("DROP TABLE feedback_asset_refs; DROP TABLE feedback_receipts; DROP TABLE feedback_events; PRAGMA user_version=19;").unwrap();
    drop(db);
    let a = Application::open(&path).unwrap();
    for (name, category) in [("A", "top"), ("裤", "bottom"), ("鞋", "shoes")] {
        a.execute(
            "clothing.create",
            json!({"member_id":"default","name":name,"category":category}),
        )
        .unwrap();
    }
    let r = a
        .execute(
            "recommendation.generate",
            json!({"member_id":"default","operation_key":"r","timezone":"UTC"}),
        )
        .unwrap();
    let f=a.execute("feedback.record",json!({"member_id":"default","operation_key":"f","intent_id":r["id"],"proposal_index":0,"reaction":"like"})).unwrap();
    drop(a);
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let mut workers = vec![];
    for n in 0..2 {
        let p = path.clone();
        let b = barrier.clone();
        let root = f["root_id"].clone();
        workers.push(std::thread::spawn(move||{let a=Application::open(&p).unwrap();b.wait();a.execute("feedback.correct",json!({"member_id":"default","root_id":root,"expected_revision":1,"operation_key":format!("correct{n}"),"reaction":"skip"}))}));
    }
    let outcomes: Vec<_> = workers.into_iter().map(|w| w.join().unwrap()).collect();
    assert_eq!(outcomes.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(
        outcomes
            .iter()
            .filter(|r| r.as_ref().is_err_and(|e| e.code == "version_conflict"))
            .count(),
        1
    );
}

// ==== 自 registry_contracts.rs 迁入的运行时/内容契约（issue #70 增量5）====

fn assert_result_matches_schema(schema: &Value, result: &Value, context: &str) {
    assert_eq!(
        schema["type"].as_str(),
        Some("object"),
        "{context}: outputSchema 须为 object"
    );
    let props = schema["properties"]
        .as_object()
        .unwrap_or_else(|| panic!("{context}: outputSchema 须声明 properties"));
    let required: Vec<&str> = schema["required"]
        .as_array()
        .map(|r| r.iter().map(|v| v.as_str().unwrap()).collect())
        .unwrap_or_default();
    let actual = result
        .as_object()
        .unwrap_or_else(|| panic!("{context} 结果须为对象"));
    for (key, value) in actual {
        let prop = props
            .get(key)
            .unwrap_or_else(|| panic!("{context}: 实际结果字段 {key} 未在 outputSchema 声明"));
        if let Some(const_value) = prop.get("const") {
            assert_eq!(value, const_value, "{context}: 字段 {key} 须等于 const");
        }
        if let Some(allowed) = prop["enum"].as_array() {
            assert!(allowed.contains(value), "{context}: 字段 {key} 须为枚举值");
        }
        let declared_types: Vec<String> = match prop.get("type") {
            Some(Value::String(t)) => vec![t.clone()],
            Some(Value::Array(ts)) => ts.iter().map(|v| v.as_str().unwrap().to_string()).collect(),
            _ => vec![],
        };
        let actual_type = match value {
            Value::String(_) => "string",
            Value::Number(n) if n.is_i64() || n.is_u64() => "integer",
            Value::Number(_) => "number",
            Value::Array(_) => "array",
            Value::Object(_) => "object",
            Value::Bool(_) => "boolean",
            Value::Null => "null",
        };
        assert!(
            declared_types.is_empty()
                || declared_types.iter().any(|t| {
                    t == actual_type
                        || (t == "number" && value.is_number())
                        || (t == "integer" && value.as_f64().is_some_and(|n| n.fract() == 0.0))
                }),
            "{context}: 字段 {key} 声明类型不符"
        );
    }
    for key in required {
        assert!(
            actual.contains_key(key),
            "{context}: 缺少 required 字段 {key}"
        );
    }
}


#[test]
fn feedback_family_effects_examples_and_runtime_output_contract() {
    // 真实运行时输出：反馈全生命周期；普通反馈永不产生穿着，actual_wore显式确认才产生
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("fb.sqlite")).unwrap();
    let member = app.execute("space.default", json!({})).unwrap()["member_id"]
        .as_str()
        .unwrap()
        .to_string();
    let _top = app
        .execute(
            "clothing.create",
            json!({"member_id":member,"name":"上衣","category":"top"}),
        )
        .unwrap();
    let _bottom = app
        .execute(
            "clothing.create",
            json!({"member_id":member,"name":"裤子","category":"bottom"}),
        )
        .unwrap();
    let generated = app
        .execute(
            "recommendation.generate",
            json!({"member_id":member,"operation_key":"fb-rec-1","timezone":"UTC","strategy":"rules","limit":3,"occasion":"daily"}),
        )
        .unwrap();
    assert!(!generated["proposals"].as_array().unwrap().is_empty());
    let intent_id = generated["id"].as_str().unwrap().to_string();

    let descriptors = operations().as_array().unwrap().clone();
    let schema_of = |name: &str| -> Value {
        let d = descriptors
            .iter()
            .find(|d| d["name"] == json!(name))
            .unwrap_or_else(|| panic!("{name} 必须已注册"));
        Value::Object(
            d["outputSchema"]
                .as_object()
                .unwrap_or_else(|| panic!("{name} 必须声明 outputSchema"))
                .clone(),
        )
    };

    let recorded = app
        .execute(
            "feedback.record",
            json!({"member_id":member,"operation_key":"fb-1","intent_id":intent_id,"proposal_index":0,"reaction":"like"}),
        )
        .unwrap();
    assert_eq!(recorded["revoked"], json!(false));
    assert_eq!(recorded["wear_event_id"], Value::Null, "普通反馈不产生穿着");
    assert_result_matches_schema(&schema_of("feedback.record"), &recorded, "record");
    let got = app
        .execute(
            "feedback.get",
            json!({"member_id":member,"id":recorded["id"]}),
        )
        .unwrap();
    assert_eq!(got["id"], recorded["id"]);
    assert_result_matches_schema(&schema_of("feedback.get"), &got, "get");
    let listed = app
        .execute("feedback.list", json!({"member_id":member}))
        .unwrap();
    assert_result_matches_schema(&schema_of("feedback.list"), &listed, "list");
    assert_eq!(listed["items"].as_array().unwrap().len(), 1);

    let corrected = app
        .execute(
            "feedback.correct",
            json!({"member_id":member,"root_id":recorded["root_id"],"operation_key":"fb-2","expected_revision":1,"reaction":"skip"}),
        )
        .unwrap();
    assert_eq!(corrected["revision"], json!(2));
    assert_eq!(corrected["supersedes_event_id"], recorded["id"]);
    assert_result_matches_schema(&schema_of("feedback.correct"), &corrected, "correct");
    let history = app
        .execute(
            "feedback.history",
            json!({"member_id":member,"root_id":recorded["root_id"]}),
        )
        .unwrap();
    assert_result_matches_schema(&schema_of("feedback.history"), &history, "history");
    assert_eq!(history["items"].as_array().unwrap().len(), 2);
    assert_eq!(history["next_cursor"], Value::Null);
    let conflict = app
        .execute(
            "feedback.correct",
            json!({"member_id":member,"root_id":recorded["root_id"],"operation_key":"fb-3","expected_revision":1,"reaction":"like"}),
        )
        .unwrap_err();
    assert_eq!(conflict.code, "version_conflict", "过期修订必须拒绝");
    let revoked = app
        .execute(
            "feedback.revoke",
            json!({"member_id":member,"root_id":recorded["root_id"],"operation_key":"fb-4","expected_revision":2,"reason":"不再喜欢"}),
        )
        .unwrap();
    assert_eq!(revoked["revoked"], json!(true));
    assert_eq!(revoked["revision"], json!(3));
    assert_result_matches_schema(&schema_of("feedback.revoke"), &revoked, "revoke");
    let listed_after = app
        .execute("feedback.list", json!({"member_id":member}))
        .unwrap();
    assert_eq!(
        listed_after["items"].as_array().unwrap().len(),
        0,
        "默认列表不含已撤销反馈"
    );

    let generated2 = app
        .execute(
            "recommendation.generate",
            json!({"member_id":member,"operation_key":"fb-rec-2","timezone":"UTC","strategy":"rules","limit":3,"occasion":"work"}),
        )
        .unwrap();
    let another = app
        .execute(
            "feedback.try_another",
            json!({"member_id":member,"operation_key":"fb-5","intent_id":generated2["id"],"proposal_index":0}),
        )
        .unwrap();
    assert_eq!(another["feedback"]["reaction"], json!("skip"));
    assert_result_matches_schema(&schema_of("feedback.try_another"), &another, "try_another");

    let wore = app
        .execute(
            "feedback.record",
            json!({"member_id":member,"operation_key":"fb-6","intent_id":intent_id,"proposal_index":0,"reaction":"actual_wore","wear":{"confirmed":true,"local_date":"2026-10-07","timezone":"UTC"}}),
        )
        .unwrap();
    assert!(
        wore["wear_event_id"].is_string(),
        "actual_wore显式确认才产生WearEvent"
    );
    assert_result_matches_schema(&schema_of("feedback.record"), &wore, "record-actual_wore");
    let wears = app
        .execute("wear.list", json!({"member_id":member}))
        .unwrap();
    assert_eq!(
        wears["items"].as_array().unwrap().len(),
        1,
        "只有实际穿着产生WearEvent"
    );

}
