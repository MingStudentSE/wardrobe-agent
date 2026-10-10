use serde_json::{json, Value};
use wardrobe_core::{operations, Application};

/// 输出契约自检：实际结果字段必须在 outputSchema 声明且类型相符。
fn assert_matches_schema(schema: &Value, result: &Value, context: &str) {
    let props = schema["properties"]
        .as_object()
        .unwrap_or_else(|| panic!("{context}: outputSchema 须声明 properties"));
    for (key, value) in result
        .as_object()
        .unwrap_or_else(|| panic!("{context} 须为对象"))
    {
        let prop = props
            .get(key)
            .unwrap_or_else(|| panic!("{context}: 实际结果字段 {key} 未在 outputSchema 声明"));
        if let Some(const_value) = prop.get("const") {
            assert_eq!(value, const_value, "{context}: 字段 {key} 须等于 const");
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
            declared_types.is_empty() || declared_types.iter().any(|t| t == &actual_type),
            "{context}: 字段 {key} 声明类型 {declared_types:?} 不符实际 {actual_type}"
        );
    }
    for key in schema["required"]
        .as_array()
        .map(|r| r.iter().map(|v| v.as_str().unwrap()).collect::<Vec<_>>())
        .unwrap_or_default()
    {
        assert!(
            result.as_object().unwrap().contains_key(key),
            "{context}: 缺少 required 字段 {key}"
        );
    }
}

fn schema_of(name: &str) -> Value {
    operations()
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["name"] == json!(name))
        .unwrap_or_else(|| panic!("{name} 必须已注册"))["outputSchema"]
        .clone()
}

fn fixture_image(dir: &std::path::Path, name: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    image::RgbImage::from_pixel(16, 16, image::Rgb([240, 240, 240]))
        .save(&path)
        .unwrap();
    path
}

fn setup() -> (tempfile::TempDir, Application) {
    let dir = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("db")).unwrap();
    (dir, app)
}

/// 候选草稿创建/读取/纠错往返：候选域独立身份，人工纠错显式发生，
/// 未知事实保持未知。
#[test]
fn candidate_draft_roundtrip_and_explicit_correction() {
    let (_dir, app) = setup();
    let created = app
        .execute(
            "purchase.candidate.create",
            json!({"member_id":"default","name":"草稿白衬衫","category":"top","color":["white"],"style":["casual"]}),
        )
        .unwrap();
    assert_matches_schema(
        &schema_of("purchase.candidate.create"),
        &created,
        "candidate.create",
    );
    assert_eq!(created["domain"], "purchase_candidate");
    assert_eq!(created["state"], "draft");
    assert_eq!(created["facts"]["name"], "草稿白衬衫");
    assert_eq!(created["facts"]["color"], json!(["white"]));
    // 未知事实保持未知：尺码与价格不编造。
    assert_eq!(created["facts"]["size_label"], Value::Null);
    assert_eq!(created["facts"]["purchase_price_minor"], Value::Null);
    let id = created["id"].as_str().unwrap().to_string();

    // 候选不在衣物库存中：列表与读取都查不到。
    let clothing = app
        .execute("clothing.list", json!({"member_id":"default","limit":100}))
        .unwrap();
    assert!(clothing["items"].as_array().unwrap().is_empty());
    let err = app
        .execute("clothing.get", json!({"member_id":"default","id":id}))
        .unwrap_err();
    assert_eq!(err.code, "not_found");

    // 幂等键不存在：无操作键的显式纠错要求确认并逐项校验。
    assert_eq!(
        app.execute("purchase.candidate.update", json!({"member_id":"default","id":id,"facts":{"category":"invented"},"confirm_correction":true})).unwrap_err().code,
        "invalid_input"
    );
    assert_eq!(
        app.execute(
            "purchase.candidate.update",
            json!({"member_id":"default","id":id,"facts":{"description":"确认过的白衬衫"}})
        )
        .unwrap_err()
        .code,
        "correction_confirmation_required"
    );
    let corrected = app
        .execute(
            "purchase.candidate.update",
            json!({"member_id":"default","id":id,"facts":{"description":"确认过的白衬衫","category":"top"},"confirm_correction":true}),
        )
        .unwrap();
    assert_matches_schema(
        &schema_of("purchase.candidate.update"),
        &corrected,
        "candidate.update",
    );
    assert_eq!(corrected["facts"]["description"], "确认过的白衬衫");
    assert_eq!(
        corrected["provenance"]["corrected_facts"]["description"],
        "确认过的白衬衫"
    );
    // 读回一致。
    let fetched = app
        .execute(
            "purchase.candidate.get",
            json!({"member_id":"default","id":id}),
        )
        .unwrap();
    assert_eq!(fetched, corrected);
    // 列表可见且只有候选。
    let listed = app
        .execute("purchase.candidate.list", json!({"member_id":"default"}))
        .unwrap();
    assert_eq!(listed["items"].as_array().unwrap().len(), 1);
    // 弃置后不可再纠错或分析。
    let dismissed = app
        .execute(
            "purchase.candidate.dismiss",
            json!({"member_id":"default","id":id}),
        )
        .unwrap();
    assert_eq!(dismissed["state"], "dismissed");
    assert_eq!(
        app.execute("purchase.candidate.update", json!({"member_id":"default","id":id,"facts":{"description":"再改"},"confirm_correction":true})).unwrap_err().code,
        "invalid_input"
    );
    assert_eq!(
        app.execute(
            "purchase.analysis.run",
            json!({"member_id":"default","candidate_id":id})
        )
        .unwrap_err()
        .code,
        "invalid_input"
    );
    // draft 筛选不再返回已弃置候选。
    let drafts = app
        .execute(
            "purchase.candidate.list",
            json!({"member_id":"default","state":"draft"}),
        )
        .unwrap();
    assert!(drafts["items"].as_array().unwrap().is_empty());
}

/// 候选与已有严格区分的结构性断言：候选写入后库存统计、洞察与推荐全部
/// 保持不变；候选不能被记为穿着。
#[test]
fn candidates_never_enter_inventory_insights_recommendation_or_wear() {
    let (_dir, app) = setup();
    let clothing = app
        .execute(
            "clothing.create",
            json!({"member_id":"default","name":"已有红T恤","category":"top","color":["red"],"occasion":["daily"]}),
        )
        .unwrap();
    let shoes = app
        .execute(
            "clothing.create",
            json!({"member_id":"default","name":"已有白鞋","category":"shoes","color":["white"],"occasion":["daily"]}),
        )
        .unwrap();
    let before_insights = app
        .execute(
            "insights.summary",
            json!({"member_id":"default","as_of_date":"2026-10-07","timezone":"Asia/Shanghai"}),
        )
        .unwrap();
    let before_recommendation = app
        .execute(
            "recommendation.generate",
            json!({"member_id":"default","operation_key":"before","timezone":"Asia/Shanghai","strategy":"rules","occasion":"daily"}),
        )
        .unwrap();

    let candidate = app
        .execute(
            "purchase.candidate.create",
            json!({"member_id":"default","name":"候选蓝衬衫","category":"top","color":["blue"]}),
        )
        .unwrap();
    app.execute(
        "purchase.analysis.run",
        json!({"member_id":"default","candidate_id":candidate["id"]}),
    )
    .unwrap();

    let after_insights = app
        .execute(
            "insights.summary",
            json!({"member_id":"default","as_of_date":"2026-10-07","timezone":"Asia/Shanghai"}),
        )
        .unwrap();
    assert_eq!(
        after_insights["inventory"]["total"], before_insights["inventory"]["total"],
        "候选不得计入库存总数"
    );
    assert_eq!(after_insights["inventory"]["total"], json!(2));
    let categories: Vec<&str> = after_insights["inventory"]["categories"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|c| c["value"].as_str())
        .collect();
    assert!(!categories.contains(&"候选蓝衬衫"));
    let after_recommendation = app
        .execute(
            "recommendation.generate",
            json!({"member_id":"default","operation_key":"after","timezone":"Asia/Shanghai","strategy":"rules","occasion":"daily"}),
        )
        .unwrap();
    let ids = |intent: &Value| -> Vec<Vec<String>> {
        intent["proposals"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| {
                p["items"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|i| i["clothing_id"].as_str().unwrap().to_string())
                    .collect::<Vec<_>>()
            })
            .collect()
    };
    assert_eq!(
        ids(&before_recommendation),
        ids(&after_recommendation),
        "候选不得进入推荐组合"
    );
    // 候选 ID 不是衣物：不能记穿着、不能编辑归档。
    for operation in ["wear.record"] {
        let err = app
            .execute(
                operation,
                json!({"member_id":"default","operation_key":"candidate-wear","local_date":"2026-10-07","timezone":"Asia/Shanghai","clothing_ids":[candidate["id"]]}),
            )
            .unwrap_err();
        assert_eq!(err.code, "not_found", "{operation} 拒绝候选身份");
    }
    assert_eq!(
        app.execute(
            "clothing.update",
            json!({"member_id":"default","id":candidate["id"],"favorite":true})
        )
        .unwrap_err()
        .code,
        "not_found"
    );
    assert_eq!(
        app.execute(
            "clothing.archive",
            json!({"member_id":"default","id":candidate["id"]})
        )
        .unwrap_err()
        .code,
        "not_found"
    );
    let _ = (clothing, shoes);
}

/// 识别收据先行与确认门槛：未确认上传直接拒绝；确认后任务收据先于任何
/// 网络请求持久化；非视觉供应商拒绝。
#[test]
fn candidate_recognition_requires_confirmation_and_vision_provider() {
    let (dir, app) = setup();
    let image = fixture_image(dir.path(), "candidate.png");
    let asset = app
        .execute("assets.import", json!({"member_id":"default","path":image}))
        .unwrap();
    let candidate = app
        .execute(
            "purchase.candidate.create",
            json!({"member_id":"default","name":"候选卫衣","category":"top"}),
        )
        .unwrap();
    let provider = app
        .execute(
            "provider.save",
            json!({"name":"无视觉","endpoint":"http://127.0.0.1:1/v1","model":"m","capabilities":["text"],"authentication":"none"}),
        )
        .unwrap();
    let base = json!({
        "member_id":"default",
        "operation_key":"candidate-vision",
        "candidate_id":candidate["id"],
        "provider_id":provider["id"],
        "asset_ids":[asset["id"]],
        "upload_role":"product_display",
    });
    assert_eq!(
        app.execute("purchase.candidate.recognize", base.clone())
            .unwrap_err()
            .code,
        "upload_confirmation_required"
    );
    let mut confirmed = base.clone();
    confirmed["confirm_upload"] = json!(true);
    assert_eq!(
        app.execute("purchase.candidate.recognize", confirmed.clone())
            .unwrap_err()
            .code,
        "unsupported_capability",
        "未声明视觉能力的供应商拒绝"
    );
    let vision = app
        .execute(
            "provider.save",
            json!({"name":"本地视觉","endpoint":"http://127.0.0.1:18093/v1","model":"candidate-fixture","capabilities":["vision"],"authentication":"none"}),
        )
        .unwrap();
    confirmed["provider_id"] = vision["id"].clone();
    let job = app
        .execute("purchase.candidate.recognize", confirmed.clone())
        .unwrap();
    assert_matches_schema(
        &schema_of("purchase.candidate.recognize"),
        &job,
        "candidate.recognize",
    );
    assert_eq!(job["kind"], "purchase_recognition");
    assert_eq!(job["state"], "queued");
    assert_eq!(job["payload"]["candidate_id"], candidate["id"]);
    // 同键同输入幂等；同键改输入冲突。
    assert_eq!(
        app.execute("purchase.candidate.recognize", confirmed.clone())
            .unwrap(),
        job
    );
    let mut changed = confirmed.clone();
    changed["asset_ids"] = json!([]);
    assert_eq!(
        app.execute("purchase.candidate.recognize", changed)
            .unwrap_err()
            .code,
        "idempotency_conflict",
        "同一操作标识不能替换发送意图"
    );
}

/// 真实 HTTP 识别往返：建议写入候选 provenance，人工纠错后重识别不覆盖
/// 人工值；候选始终不进入库存。
#[test]
fn candidate_vision_recognition_suggests_without_overwriting_manual_facts() {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}/v1", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        for _ in 0..2 {
            let (mut s, _) = listener.accept().unwrap();
            let mut b = vec![];
            loop {
                let mut chunk = [0; 4096];
                let n = s.read(&mut chunk).unwrap();
                assert!(n > 0);
                b.extend_from_slice(&chunk[..n]);
                if let Some(split) = b.windows(4).position(|w| w == b"\r\n\r\n") {
                    let h = String::from_utf8_lossy(&b[..split]);
                    let len: usize = h
                        .lines()
                        .find_map(|l| {
                            l.to_lowercase()
                                .strip_prefix("content-length: ")
                                .map(|v| v.parse().unwrap())
                        })
                        .unwrap();
                    if b.len() >= split + 4 + len {
                        let req: Value =
                            serde_json::from_slice(&b[split + 4..split + 4 + len]).unwrap();
                        assert!(req["messages"][0]["content"][1]["image_url"]["url"]
                            .as_str()
                            .unwrap()
                            .starts_with("data:image/png;base64,"));
                        let prompt = req["messages"][0]["content"][0]["text"].as_str().unwrap();
                        assert!(prompt.contains("Do not guess brand, size, material, price"));
                        break;
                    }
                }
            }
            let body = json!({"model":"candidate-fixture","choices":[{"message":{"content":"A blue shirt; exact material and size cannot be established."}}],"usage":{"prompt_tokens":25,"completion_tokens":12}}).to_string();
            write!(s, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{}", body.len(), body).unwrap();
        }
    });
    let (dir, app) = setup();
    let image = fixture_image(dir.path(), "cand.png");
    let asset = app
        .execute("assets.import", json!({"member_id":"default","path":image}))
        .unwrap();
    let candidate = app
        .execute(
            "purchase.candidate.create",
            json!({"member_id":"default","name":"手工命名候选","category":"top"}),
        )
        .unwrap();
    let vision = app
        .execute(
            "provider.save",
            json!({"name":"HTTP fixture","endpoint":endpoint,"model":"candidate-fixture","capabilities":["vision"],"authentication":"none"}),
        )
        .unwrap();
    let job = app
        .execute(
            "purchase.candidate.recognize",
            json!({"member_id":"default","operation_key":"vision-1","candidate_id":candidate["id"],"provider_id":vision["id"],"asset_ids":[asset["id"]],"upload_role":"product_display","confirm_upload":true}),
        )
        .unwrap();
    let done = app
        .execute(
            "purchase.candidate.process",
            json!({"member_id":"default","id":job["id"]}),
        )
        .unwrap();
    assert_matches_schema(
        &schema_of("purchase.candidate.process"),
        &done,
        "candidate.process",
    );
    assert_eq!(done["state"], "succeeded");
    assert_eq!(done["result"]["candidate_id"], candidate["id"]);

    let updated = app
        .execute(
            "purchase.candidate.get",
            json!({"member_id":"default","id":candidate["id"]}),
        )
        .unwrap();
    assert_eq!(updated["provenance"]["source"], "vision");
    assert_eq!(
        updated["provenance"]["suggested_facts"]["color"],
        json!(["blue"])
    );
    assert_eq!(updated["provenance"]["suggested_facts"]["category"], "top");
    assert!(updated["provenance"]["uncertainties"]["notice"]
        .as_str()
        .unwrap()
        .contains("未经核实"));
    assert_eq!(
        updated["provenance"]["recognition"]["prompt_version"],
        "candidate-visible-description-v1"
    );
    // 建议不自动覆盖人工事实；候选不进库存。
    assert_eq!(updated["facts"]["name"], "手工命名候选");
    assert_eq!(updated["facts"]["color"], json!([]));
    assert_eq!(
        app.execute("clothing.list", json!({"member_id":"default","limit":100}))
            .unwrap()["items"],
        json!([])
    );
    assert_eq!(updated["asset_ids"], json!([asset["id"]]));

    // 人工纠错后重识别：建议更新，人工纠错值保留在 corrected_facts 历史。
    app.execute(
        "purchase.candidate.update",
        json!({"member_id":"default","id":candidate["id"],"facts":{"description":"人工确认的蓝衬衫"},"confirm_correction":true}),
    )
    .unwrap();
    let job2 = app
        .execute(
            "purchase.candidate.recognize",
            json!({"member_id":"default","operation_key":"vision-2","candidate_id":candidate["id"],"provider_id":vision["id"],"asset_ids":[asset["id"]],"upload_role":"product_display","confirm_upload":true}),
        )
        .unwrap();
    app.execute(
        "purchase.candidate.process",
        json!({"member_id":"default","id":job2["id"]}),
    )
    .unwrap();
    let after = app
        .execute(
            "purchase.candidate.get",
            json!({"member_id":"default","id":candidate["id"]}),
        )
        .unwrap();
    assert_eq!(after["provenance"]["recognition"]["job_id"], job2["id"]);
    assert_eq!(
        after["provenance"]["corrected_facts"]["description"],
        "人工确认的蓝衬衫"
    );
    assert_eq!(after["facts"]["description"], "人工确认的蓝衬衫");
    server.join().unwrap();
}

/// 识别失败如实反馈：供应商拒绝时候选保持原样，任务失败可查证。
#[test]
fn candidate_recognition_failure_reports_honestly_and_leaves_candidate_intact() {
    let (dir, app) = setup();
    let image = fixture_image(dir.path(), "fail.png");
    let asset = app
        .execute("assets.import", json!({"member_id":"default","path":image}))
        .unwrap();
    let candidate = app
        .execute(
            "purchase.candidate.create",
            json!({"member_id":"default","name":"候选外套","category":"outerwear"}),
        )
        .unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}/v1", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        if let Ok((mut s, _)) = listener.accept() {
            let body = json!({"error":{"message":"rejected"}}).to_string();
            let _ = write_response(&mut s, 403, &body);
        }
    });
    let vision = app
        .execute(
            "provider.save",
            json!({"name":"拒绝fixture","endpoint":endpoint,"model":"reject-fixture","capabilities":["vision"],"authentication":"none"}),
        )
        .unwrap();
    let job = app
        .execute(
            "purchase.candidate.recognize",
            json!({"member_id":"default","operation_key":"vision-fail","candidate_id":candidate["id"],"provider_id":vision["id"],"asset_ids":[asset["id"]],"upload_role":"product_display","confirm_upload":true}),
        )
        .unwrap();
    let done = app
        .execute(
            "purchase.candidate.process",
            json!({"member_id":"default","id":job["id"]}),
        )
        .unwrap();
    assert_eq!(done["state"], "failed");
    assert_eq!(done["error"]["code"], "provider_rejected");
    assert!(done["error"]["message"]
        .as_str()
        .is_some_and(|m| m.contains("仍可手工填写") || m.contains("候选")));
    let after = app
        .execute(
            "purchase.candidate.get",
            json!({"member_id":"default","id":candidate["id"]}),
        )
        .unwrap();
    assert_eq!(after["provenance"]["source"], "manual");
    assert!(after["provenance"]["suggested_facts"].is_null());
    // 已提交失败但未到达 received：供应商明确拒绝不属于未知提交，可显式重试。
}

fn write_response(
    stream: &mut std::net::TcpStream,
    status: u16,
    body: &str,
) -> std::io::Result<()> {
    use std::io::Write;
    let text = match status {
        403 => "403 Forbidden",
        _ => "200 OK",
    };
    write!(
        stream,
        "HTTP/1.1 {}\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{}",
        text,
        body.len(),
        body
    )
}

/// 搭配潜力引用真实库存：组合条目里的库存件全部是真实可用 clothing_id，
/// 候选条目标记 candidate:true；缺角色时如实列出。
#[test]
fn pairing_potential_cites_real_inventory_and_reports_missing_roles() {
    let (_dir, app) = setup();
    let top = app
        .execute(
            "clothing.create",
            json!({"member_id":"default","name":"库存白T","category":"top","color":["white"],"occasion":["daily"]}),
        )
        .unwrap();
    let shoes = app
        .execute(
            "clothing.create",
            json!({"member_id":"default","name":"库存黑鞋","category":"shoes","color":["black"],"occasion":["daily"]}),
        )
        .unwrap();
    let candidate = app
        .execute(
            "purchase.candidate.create",
            json!({"member_id":"default","name":"候选牛仔裤","category":"bottom","color":["blue"]}),
        )
        .unwrap();
    let analysis = app
        .execute(
            "purchase.analysis.run",
            json!({"member_id":"default","candidate_id":candidate["id"]}),
        )
        .unwrap();
    assert_matches_schema(
        &schema_of("purchase.analysis.run"),
        &analysis,
        "analysis.run",
    );
    let pairing = &analysis["pairing_potential"];
    assert_eq!(pairing["role"], "bottom");
    assert_eq!(pairing["combinations_total"], json!(1));
    let proposal = &pairing["proposals"][0];
    assert_eq!(proposal["complete"], true);
    let items = proposal["items"].as_array().unwrap();
    let candidate_items: Vec<&Value> = items.iter().filter(|i| i["candidate"] == true).collect();
    assert_eq!(candidate_items.len(), 1);
    assert_eq!(candidate_items[0]["candidate_id"], candidate["id"]);
    assert!(candidate_items[0]["clothing_id"].is_null());
    let stock_ids: Vec<&str> = items
        .iter()
        .filter(|i| i["candidate"] == false)
        .filter_map(|i| i["clothing_id"].as_str())
        .collect();
    assert_eq!(stock_ids.len(), 2);
    assert!(stock_ids.contains(&top["id"].as_str().unwrap()));
    assert!(stock_ids.contains(&shoes["id"].as_str().unwrap()));
    // 洗护不可用的库存不进入搭配组合。
    app.execute(
        "care.mark_dirty",
        json!({"member_id":"default","operation_key":"dirty-top","clothing_id":top["id"],"dirty":true,"local_date":"2026-10-07","timezone":"Asia/Shanghai"}),
    )
    .unwrap();
    let reanalysis = app
        .execute(
            "purchase.analysis.run",
            json!({"member_id":"default","candidate_id":candidate["id"]}),
        )
        .unwrap();
    let items = reanalysis["pairing_potential"]["proposals"][0]["items"]
        .as_array()
        .unwrap();
    assert!(
        !items.iter().any(|i| i["clothing_id"] == top["id"]),
        "洗护待洗单品不进入搭配组合"
    );
    assert_eq!(
        reanalysis["pairing_potential"]["proposals"][0]["missing_roles"],
        json!(["top"])
    );
    assert_eq!(
        reanalysis["pairing_potential"]["proposals"][0]["complete"],
        false
    );
}

/// 重复购买分析：同类别比较、确定性评分、跨运行一致；不同类别与未知颜色
/// 不猜测。
#[test]
fn duplicate_risk_is_deterministic_and_honest_about_unknowns() {
    let (dir, app) = setup();
    // 两件同类别高重合 + 一件低重合 + 一件不同类别。
    for (name, color, style) in [
        ("重复白衬衫A", "white", "formal"),
        ("重复白衬衫B", "white", "formal"),
        ("花哨衬衫", "red", "sport"),
    ] {
        app.execute(
            "clothing.create",
            json!({"member_id":"default","name":name,"category":"top","color":[color],"style":[style],"seasons":["spring"]}),
        )
        .unwrap();
    }
    app.execute(
        "clothing.create",
        json!({"member_id":"default","name":"库内黑裤","category":"bottom"}),
    )
    .unwrap();
    let candidate = app
        .execute(
            "purchase.candidate.create",
            json!({"member_id":"default","name":"候选白衬衫","category":"top","color":["white"],"style":["formal"],"seasons":["spring","autumn"]}),
        )
        .unwrap();
    let first = app
        .execute(
            "purchase.analysis.run",
            json!({"member_id":"default","candidate_id":candidate["id"]}),
        )
        .unwrap();
    let second = app
        .execute(
            "purchase.analysis.run",
            json!({"member_id":"default","candidate_id":candidate["id"]}),
        )
        .unwrap();
    // 确定性：两次运行除版本与ID外数字完全一致。
    assert_eq!(first["duplicate_risk"], second["duplicate_risk"]);
    assert_eq!(second["version"], json!(2));
    let risk = &first["duplicate_risk"];
    assert_eq!(risk["available"], true);
    assert_eq!(risk["compared_total"], json!(3));
    assert_eq!(risk["high_overlap_total"], json!(2));
    let items = risk["items"].as_array().unwrap();
    assert_eq!(items.len(), 3);
    // 排序：分数降序。
    let scores: Vec<u64> = items.iter().map(|i| i["score"].as_u64().unwrap()).collect();
    let mut sorted = scores.clone();
    sorted.sort_unstable_by(|a, b| b.cmp(a));
    assert_eq!(scores, sorted);
    // 高重合项引用真实库存并带匹配维度。
    let high: Vec<&Value> = items.iter().filter(|i| i["high_overlap"] == true).collect();
    assert_eq!(high.len(), 2);
    assert_eq!(high[0]["matched"]["color"], json!(["white"]));
    // 独立第二个数据库同样输入 → 相同分数（跨库确定性）。
    drop(app);
    let (_dir2, app2) = setup();
    let _ = &dir;
    for (name, color, style) in [
        ("重复白衬衫A", "white", "formal"),
        ("重复白衬衫B", "white", "formal"),
        ("花哨衬衫", "red", "sport"),
    ] {
        app2.execute(
            "clothing.create",
            json!({"member_id":"default","name":name,"category":"top","color":[color],"style":[style],"seasons":["spring"]}),
        )
        .unwrap();
    }
    let candidate2 = app2
        .execute(
            "purchase.candidate.create",
            json!({"member_id":"default","name":"候选白衬衫","category":"top","color":["white"],"style":["formal"],"seasons":["spring","autumn"]}),
        )
        .unwrap();
    let other = app2
        .execute(
            "purchase.analysis.run",
            json!({"member_id":"default","candidate_id":candidate2["id"]}),
        )
        .unwrap();
    let strip_ids = |items: &Value| -> Vec<(u64, bool, Value)> {
        items
            .as_array()
            .unwrap()
            .iter()
            .map(|i| {
                (
                    i["score"].as_u64().unwrap(),
                    i["high_overlap"] == true,
                    i["matched"].clone(),
                )
            })
            .collect()
    };
    assert_eq!(
        strip_ids(&other["duplicate_risk"]["items"]),
        strip_ids(&risk["items"])
    );
    assert_eq!(
        other["duplicate_risk"]["high_overlap_total"],
        risk["high_overlap_total"]
    );
}

/// 缺口分析：0 数量类别/场景为空洞；候选填补声明只在精确相等时作出；
/// 未记录标签如实列为不确定性。
#[test]
fn gap_analysis_reports_holes_and_exact_candidate_fills() {
    let (_dir, app) = setup();
    // 只有 top 与 shoes；bottom 空洞；没有任何 formal 标签。
    app.execute(
        "clothing.create",
        json!({"member_id":"default","name":"库存T","category":"top","occasion":["daily"]}),
    )
    .unwrap();
    app.execute(
        "clothing.create",
        json!({"member_id":"default","name":"库存鞋","category":"shoes"}),
    )
    .unwrap();
    let candidate = app
        .execute(
            "purchase.candidate.create",
            json!({"member_id":"default","name":"候选西裤","category":"bottom","occasion":["formal"]}),
        )
        .unwrap();
    let analysis = app
        .execute(
            "purchase.analysis.run",
            json!({"member_id":"default","candidate_id":candidate["id"]}),
        )
        .unwrap();
    let gap = &analysis["gap_analysis"];
    let hole_categories: Vec<&str> = gap["categories"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["hole"] == true)
        .filter_map(|c| c["category"].as_str())
        .collect();
    assert!(hole_categories.contains(&"bottom"));
    assert!(!hole_categories.contains(&"top"));
    let formal = gap["occasions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["occasion"] == "formal")
        .unwrap();
    assert_eq!(formal["hole"], true);
    assert_eq!(formal["count"], json!(0));
    // 候选填补声明精确。
    assert_eq!(gap["candidate_fills"]["categories"], json!(["bottom"]));
    assert_eq!(gap["candidate_fills"]["occasions"], json!(["formal"]));
    // 未记标签的鞋计入不确定性。
    assert!(analysis["uncertainties"]
        .as_array()
        .unwrap()
        .iter()
        .any(|u| u.as_str().unwrap().contains("未记录任何场合标签")));
    // 已有库存的类别不作出填补声明。
    let another = app
        .execute(
            "purchase.candidate.create",
            json!({"member_id":"default","name":"候选又一件T","category":"top"}),
        )
        .unwrap();
    let analysis2 = app
        .execute(
            "purchase.analysis.run",
            json!({"member_id":"default","candidate_id":another["id"]}),
        )
        .unwrap();
    assert_eq!(
        analysis2["gap_analysis"]["candidate_fills"]["categories"],
        json!([])
    );
    assert!(analysis2["uncertainties"]
        .as_array()
        .unwrap()
        .iter()
        .any(|u| u.as_str().unwrap().contains("不填补类别空洞")));
}

/// 缺数据时的不确定性：类别未知的候选不能获得编造的搭配/重复结论；
/// 自定义类别如实说明。
#[test]
fn unknown_or_custom_candidate_categories_stay_unknown() {
    let (_dir, app) = setup();
    app.execute(
        "clothing.create",
        json!({"member_id":"default","name":"库存T","category":"top"}),
    )
    .unwrap();
    // category 是必填，但自定义类别允许。
    let custom = app
        .execute(
            "category.create",
            json!({"member_id":"default","name":"户外"}),
        )
        .unwrap();
    let candidate = app
        .execute(
            "purchase.candidate.create",
            json!({"member_id":"default","name":"候选冲锋衣","category":custom["id"],"color":["orange"]}),
        )
        .unwrap();
    let analysis = app
        .execute(
            "purchase.analysis.run",
            json!({"member_id":"default","candidate_id":candidate["id"]}),
        )
        .unwrap();
    assert_eq!(analysis["pairing_potential"]["role"], Value::Null);
    assert_eq!(analysis["pairing_potential"]["proposals"], json!([]));
    // 自定义类别仍可与同类别库存确定性比较（此处库存为 0 件），但搭配角色
    // 无法映射、如实说明。
    assert_eq!(analysis["duplicate_risk"]["available"], true);
    assert_eq!(analysis["duplicate_risk"]["compared_total"], json!(0));
    assert_eq!(analysis["duplicate_risk"]["high_overlap_total"], json!(0));
    assert!(analysis["uncertainties"]
        .as_array()
        .unwrap()
        .iter()
        .any(|u| u.as_str().unwrap().contains("自定义类别")));
}

/// 分析结果版本化保留：历史可查、可按候选筛选、可交叉核对真实库存 ID。
#[test]
fn analysis_results_are_versioned_and_viewable() {
    let (_dir, app) = setup();
    let item = app
        .execute(
            "clothing.create",
            json!({"member_id":"default","name":"库存白衬衫","category":"top","color":["white"]}),
        )
        .unwrap();
    let candidate = app
        .execute(
            "purchase.candidate.create",
            json!({"member_id":"default","name":"候选白衬衫","category":"top","color":["white"]}),
        )
        .unwrap();
    let first = app
        .execute(
            "purchase.analysis.run",
            json!({"member_id":"default","candidate_id":candidate["id"]}),
        )
        .unwrap();
    let second = app
        .execute(
            "purchase.analysis.run",
            json!({"member_id":"default","candidate_id":candidate["id"]}),
        )
        .unwrap();
    let listed = app
        .execute(
            "purchase.analysis.list",
            json!({"member_id":"default","candidate_id":candidate["id"]}),
        )
        .unwrap();
    assert_matches_schema(
        &schema_of("purchase.analysis.list"),
        &listed,
        "analysis.list",
    );
    let items = listed["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(first["version"], json!(1));
    assert_eq!(second["version"], json!(2));
    let fetched = app
        .execute(
            "purchase.analysis.get",
            json!({"member_id":"default","id":first["id"]}),
        )
        .unwrap();
    assert_matches_schema(
        &schema_of("purchase.analysis.get"),
        &fetched,
        "analysis.get",
    );
    assert_eq!(fetched, first);
    // 引用的库存 ID 可交叉核对。
    let cited: Vec<&str> = second["duplicate_risk"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|i| i["clothing_id"].as_str())
        .collect();
    assert!(cited.contains(&item["id"].as_str().unwrap()));
    assert_eq!(
        app.execute("clothing.get", json!({"member_id":"default","id":cited[0]}))
            .unwrap()["id"],
        json!(cited[0])
    );
}

/// 语义红线：候选域不提供自动购买操作；分析输出显式携带不计数、不自动
/// 购买的恒定标记。
#[test]
fn purchase_domain_has_no_auto_buy_and_analysis_flags_are_honest() {
    let names: Vec<String> = operations()
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|d| d["name"].as_str().map(str::to_owned))
        .collect();
    for name in names.iter().filter(|n| n.starts_with("purchase.")) {
        let lower = name.to_lowercase();
        for forbidden in ["buy", "checkout", "cart", "pay"] {
            assert!(
                !lower.contains(forbidden),
                "候选域不得出现自动购买语义操作：{name}"
            );
        }
    }
    assert!(names.iter().any(|n| n == "purchase.analysis.run"));
    assert!(names.iter().any(|n| n == "purchase.candidate.create"));
    let (_dir, app) = setup();
    let candidate = app
        .execute(
            "purchase.candidate.create",
            json!({"member_id":"default","name":"候选","category":"top"}),
        )
        .unwrap();
    let analysis = app
        .execute(
            "purchase.analysis.run",
            json!({"member_id":"default","candidate_id":candidate["id"]}),
        )
        .unwrap();
    assert_eq!(analysis["creates_wear"], false);
    assert_eq!(analysis["counts_toward_inventory"], false);
    assert_eq!(analysis["auto_purchase"], false);
}

/// 列表读取有上限（review P2-6）：candidate.list / analysis.list 与
/// scenario.list 对齐同一上限常量，绝不无限回包。
#[test]
fn candidate_and_analysis_lists_are_capped() {
    let (_dir, app) = setup();
    const LIMIT: usize = 50;
    for i in 0..(LIMIT + 2) {
        app.execute(
            "purchase.candidate.create",
            json!({"member_id":"default","name":format!("候选{i:03}"),"category":"top"}),
        )
        .unwrap();
    }
    let listed = app
        .execute("purchase.candidate.list", json!({"member_id":"default"}))
        .unwrap();
    let items = listed["items"].as_array().unwrap();
    assert_eq!(items.len(), LIMIT, "候选列表最多返回上限条数: {listed}");
    // 倒序：最新创建的在前，最旧的被上限截断。
    assert_eq!(items[0]["facts"]["name"], json!("候选051"));
    let states: Vec<&str> = items.iter().filter_map(|d| d["state"].as_str()).collect();
    assert!(states.iter().all(|s| *s == "draft"));

    // 分析列表同上限：对同一候选运行上限+2 次，只返回最近上限条。
    let candidate_id = items[0]["id"].as_str().unwrap().to_string();
    let mut last_version = 0u64;
    for _ in 0..(LIMIT + 2) {
        let analysis = app
            .execute(
                "purchase.analysis.run",
                json!({"member_id":"default","candidate_id":candidate_id}),
            )
            .unwrap();
        last_version = analysis["version"].as_u64().unwrap();
    }
    assert_eq!(last_version, (LIMIT + 2) as u64, "每次分析版本递增");
    let analyses = app
        .execute(
            "purchase.analysis.list",
            json!({"member_id":"default","candidate_id":candidate_id}),
        )
        .unwrap();
    let analysis_items = analyses["items"].as_array().unwrap();
    assert_eq!(analysis_items.len(), LIMIT, "分析列表最多返回上限条数");
    assert_eq!(
        analysis_items[0]["version"],
        json!(last_version),
        "倒序：最新版本在前"
    );
}
