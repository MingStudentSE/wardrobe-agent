use serde_json::{json, Value};
use wardrobe_core::Application;

// F17 场景扩展（ticket-23, issue #23）：婚礼/面试单事件与多日旅行计划、去重打包清单。
// 公开 Application API 回归；场景计划是领域工件（creates_wear:false），
// 结构性不进入 wear 面：所有断言先验证外部行为，再验证 wear.list 为空。
// 模型部分沿用 #22 确立的本地脚本化 HTTP oracle（真实模型成功需用户自配密钥，
// 见 docs/verification/ticket-23 与本文件内的如实标注）。

fn open_app(directory: &std::path::Path) -> Application {
    Application::open(&directory.join("db")).unwrap()
}

fn bootstrap_member(app: &Application) -> String {
    app.execute("space.default", json!({})).unwrap()["member_id"]
        .as_str()
        .expect("workspace member id")
        .to_owned()
}

/// 通过真实 clothing.create 造库存；返回创建文档列表。
fn seed(app: &Application, member: &str, items: &[(&str, &str)]) -> Vec<Value> {
    items
        .iter()
        .map(|(name, category)| {
            app.execute(
                "clothing.create",
                json!({"member_id":member,"name":name,"category":category}),
            )
            .unwrap()
        })
        .collect()
}

fn ids(created: &[Value]) -> Vec<String> {
    created
        .iter()
        .map(|item| item["id"].as_str().unwrap().to_owned())
        .collect()
}

fn day_clothing(plan: &Value, index: usize) -> Vec<String> {
    plan["days"]
        .as_array()
        .unwrap()
        .get(index)
        .map(|day| {
            day["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(|item| item["clothing_id"].as_str().unwrap().to_owned())
                .collect()
        })
        .unwrap_or_default()
}

fn assert_no_wear(app: &Application, member: &str) {
    let wears = app.execute("wear.list", json!({"member_id":member})).unwrap();
    assert_eq!(
        wears["items"],
        json!([]),
        "场景计划与保存绝不产生真实穿着事件"
    );
}

// ---------------------------------------------------------------------------
// 单事件（婚礼/面试）
// ---------------------------------------------------------------------------

#[test]
fn wedding_event_plan_completes_from_real_inventory_and_never_records_wear() {
    let d = tempfile::tempdir().unwrap();
    let app = open_app(d.path());
    let member = bootstrap_member(&app);
    let created = seed(
        &app,
        &member,
        &[
            ("深蓝礼服连衣裙", "dress"),
            ("黑色尖头高跟鞋", "shoes"),
            ("手拿包", "bag"),
            ("日常白T", "top"),
        ],
    );
    let dress = created[0]["id"].as_str().unwrap();
    let shoes = created[1]["id"].as_str().unwrap();

    let plan = app
        .execute(
            "scenario.plan",
            json!({
                "member_id":member,
                "operation_key":"wedding-plan-1",
                "timezone":"UTC",
                "kind":"event",
                "occasion":"wedding",
                "title":"表姐的婚礼",
                "start_date":"2026-10-10",
                "fixed_ids":[dress],
                "excluded_ids":[created[3]["id"].as_str().unwrap()],
            }),
        )
        .expect("RED(ticket-23): scenario.plan 尚未实现，这是预期的真实失败点");

    assert_eq!(plan["schema_version"], 1);
    assert_eq!(plan["member_id"], json!(member));
    assert_eq!(plan["kind"], "event");
    assert_eq!(plan["occasion"], "wedding");
    assert_eq!(plan["status"], "active");
    assert_eq!(plan["version"], 1);
    assert_eq!(plan["creates_wear"], false, "计划是领域工件，不是穿着");
    assert_eq!(plan["days_count"], 1);
    assert_eq!(plan["start_date"], "2026-10-10");
    let days = plan["days"].as_array().unwrap();
    assert_eq!(days.len(), 1);
    assert_eq!(days[0]["index"], 0);
    assert_eq!(days[0]["date"], "2026-10-10");
    assert_eq!(days[0]["complete"], true, "真实库存足够时单事件方案应完整");
    let day_ids = day_clothing(&plan, 0);
    assert!(
        day_ids.iter().any(|id| id == dress),
        "固定单品必须出现在方案中"
    );
    assert!(
        day_ids.iter().any(|id| id == shoes),
        "基础角色（鞋）必须覆盖"
    );
    assert!(
        !day_ids.iter().any(|id| id == created[3]["id"].as_str().unwrap()),
        "被排除单品绝不出现"
    );
    // 去重打包清单：单事件与当天方案同集
    let packing = &plan["packing"];
    assert_eq!(packing["total"], day_ids.len() as u64);
    let packed: Vec<String> = packing["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["clothing_id"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(packed.len(), day_ids.len(), "打包清单必须按衣物去重");
    assert!(
        packing["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|item| item["days"] == json!([0])),
        "单事件打包条目归属唯一一天"
    );
    assert!(
        plan["generation"]["source"] == json!("rules"),
        "默认本地规则生成"
    );
    // 持久化：get/list 可查看；同键重放返回同一计划
    let got = app
        .execute("scenario.get", json!({"member_id":member,"id":plan["id"]}))
        .unwrap();
    assert_eq!(got["id"], plan["id"]);
    assert_eq!(got["version"], plan["version"]);
    let listed = app
        .execute("scenario.list", json!({"member_id":member}))
        .unwrap();
    assert!(
        listed["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["id"] == plan["id"]),
        "列表须包含已保存计划"
    );
    let replay = app
        .execute(
            "scenario.plan",
            json!({
                "member_id":member,
                "operation_key":"wedding-plan-1",
                "timezone":"UTC",
                "kind":"event",
                "occasion":"wedding",
                "title":"表姐的婚礼",
                "start_date":"2026-10-10",
                "fixed_ids":[dress],
                "excluded_ids":[created[3]["id"].as_str().unwrap()],
            }),
        )
        .unwrap();
    assert_eq!(replay["id"], plan["id"], "同 operation_key 幂等返回同一计划");
    assert_no_wear(&app, &member);
}

#[test]
fn interview_plan_reports_shortage_instead_of_fabricating() {
    let d = tempfile::tempdir().unwrap();
    let app = open_app(d.path());
    let member = bootstrap_member(&app);
    // 只有一件上衣、没有鞋和下装：面试方案必须如实说明缺什么
    let created = seed(&app, &member, &[("白衬衫", "top")]);
    let plan = app
        .execute(
            "scenario.plan",
            json!({
                "member_id":member,
                "operation_key":"interview-short",
                "timezone":"UTC",
                "kind":"event",
                "occasion":"interview",
                "start_date":"2026-10-10",
                "excluded_ids":[created[0]["id"].as_str().unwrap()],
            }),
        )
        .unwrap();
    assert_eq!(
        plan["days"][0]["complete"],
        json!(false),
        "排除唯一上衣后方案不完整"
    );
    let reasons = plan["shortage_reasons"].as_array().unwrap();
    assert!(
        !reasons.is_empty(),
        "库存不足必须给出明确的无解/短缺原因"
    );
    assert!(
        reasons
            .iter()
            .any(|reason| reason.as_str().unwrap_or("").contains("缺少")),
        "短缺原因须点名缺少的角色：{reasons:?}"
    );
    assert_eq!(day_clothing(&plan, 0), Vec::<String>::new(), "不得虚构衣物");
    assert_no_wear(&app, &member);
}

// ---------------------------------------------------------------------------
// 固定/排除/归属/可用性的硬校验
// ---------------------------------------------------------------------------

#[test]
fn fixed_and_excluded_and_ownership_are_server_validated() {
    let d = tempfile::tempdir().unwrap();
    let app = open_app(d.path());
    let member = bootstrap_member(&app);
    let other = app.execute("member.create", json!({"name":"家人"})).unwrap();
    let other_id = other["member_id"].as_str().unwrap().to_string();
    let mine = seed(
        &app,
        &member,
        &[("西装外套", "outerwear"), ("西裤", "bottom"), ("皮鞋", "shoes")],
    );
    let theirs = seed(&app, &other_id, &[("别人的领带", "accessory")]);
    let suit = mine[0]["id"].as_str().unwrap();

    // 固定+排除冲突
    let conflict = app.execute(
        "scenario.plan",
        json!({"member_id":member,"operation_key":"fx-conflict","timezone":"UTC","kind":"event",
               "occasion":"interview","fixed_ids":[suit],"excluded_ids":[suit]}),
    );
    assert_eq!(
        conflict.unwrap_err().code,
        "constraint_conflict",
        "固定单品不能同时排除"
    );

    // 跨成员固定：归属校验拒绝
    let foreign = app.execute(
        "scenario.plan",
        json!({"member_id":member,"operation_key":"fx-foreign","timezone":"UTC","kind":"event",
               "occasion":"interview","fixed_ids":[theirs[0]["id"].as_str().unwrap()]}),
    );
    assert_eq!(foreign.unwrap_err().code, "not_found", "不能使用其他成员的单品");

    // 固定单品待洗（手动待洗）：可用性校验拒绝
    app.execute(
        "care.mark_dirty",
        json!({"member_id":member,"operation_key":"mark-1","clothing_id":suit,"local_date":"2026-10-07","timezone":"UTC","dirty":true}),
    )
    .unwrap();
    let dirty = app.execute(
        "scenario.plan",
        json!({"member_id":member,"operation_key":"fx-dirty","timezone":"UTC","kind":"event",
               "occasion":"interview","fixed_ids":[suit]}),
    );
    let error = dirty.unwrap_err();
    assert_eq!(
        error.code,
        "constraint_conflict",
        "待洗/不可用的固定单品必须拒绝"
    );
    assert_no_wear(&app, &member);
}

// ---------------------------------------------------------------------------
// 多日旅行：跨天去重与打包清单
// ---------------------------------------------------------------------------

#[test]
fn travel_plan_rotates_days_and_dedupes_packing_list() {
    let d = tempfile::tempdir().unwrap();
    let app = open_app(d.path());
    let member = bootstrap_member(&app);
    let created = seed(
        &app,
        &member,
        &[
            ("白色T恤", "top"),      // t1
            ("蓝色衬衫", "top"),     // t2
            ("黑色长裤", "bottom"),  // b1
            ("米色长裤", "bottom"),  // b2
            ("步行鞋", "shoes"),     // s1
            ("薄外套", "outerwear"), // o1
        ],
    );
    let all = ids(&created);
    let plan = app
        .execute(
            "scenario.plan",
            json!({
                "member_id":member,
                "operation_key":"travel-3d",
                "timezone":"UTC",
                "kind":"travel",
                "title":"三日出行",
                "start_date":"2026-10-10",
                "days":3,
                "weather":{"mode":"manual","temperature_c":8}
            }),
        )
        .expect("RED(ticket-23): 多日旅行计划尚未实现");
    assert_eq!(plan["kind"], "travel");
    assert_eq!(plan["days_count"], 3);
    let days = plan["days"].as_array().unwrap();
    assert_eq!(days.len(), 3);
    // 日期连续
    assert_eq!(days[0]["date"], "2026-10-10");
    assert_eq!(days[1]["date"], "2026-10-11");
    assert_eq!(days[2]["date"], "2026-10-12");

    let d0 = day_clothing(&plan, 0);
    let d1 = day_clothing(&plan, 1);
    let d2 = day_clothing(&plan, 2);
    for (index, day) in [(0, &d0), (1, &d1), (2, &d2)] {
        let top_count = day
            .iter()
            .filter(|id| id.as_str() == all[0] || id.as_str() == all[1])
            .count();
        let bottom_count = day
            .iter()
            .filter(|id| id.as_str() == all[2] || id.as_str() == all[3])
            .count();
        assert_eq!(top_count, 1, "第{index}天须有一件上衣");
        assert_eq!(bottom_count, 1, "第{index}天须有一件下装");
        assert!(
            day.iter().any(|id| id == &all[4]),
            "第{index}天须有鞋（鞋可跨天复用）"
        );
        assert!(
            day.iter().any(|id| id == &all[5]),
            "低温（8°C）每天须要求外层"
        );
    }
    // 贴身衣物不连续两天重复：day1 与 day0 的 top/bottom 必须不同
    for kind in [all[0].clone(), all[1].clone(), all[2].clone(), all[3].clone()] {
        assert!(
            !d0.contains(&kind) || !d1.contains(&kind),
            "上衣/下装不得连续两天重复（day0/day1）：{kind}"
        );
        assert!(
            !d1.contains(&kind) || !d2.contains(&kind),
            "上衣/下装不得连续两天重复（day1/day2）：{kind}"
        );
    }
    // day2 允许回到 day0 的衣物（隔天轮换）→ 打包清单去重
    let packing = plan["packing"]["items"].as_array().unwrap();
    assert_eq!(
        plan["packing"]["total"],
        json!(packing.len() as u64),
        "打包清单总数与条目一致（已去重）"
    );
    let shoes_entry = packing
        .iter()
        .find(|item| item["clothing_id"] == created[4]["id"])
        .expect("鞋必须出现在打包清单");
    assert_eq!(
        shoes_entry["days"],
        json!([0, 1, 2]),
        "鞋跨天复用，打包清单只出现一次并列出各天"
    );
    assert!(
        packing
            .iter()
            .all(|item| item["days"].as_array().unwrap().len() == 1
                || item["reused"] == json!(true)),
        "重复使用的条目必须带 reused 标记"
    );
    let mut packed_ids: Vec<&str> = packing
        .iter()
        .map(|item| item["clothing_id"].as_str().unwrap())
        .collect();
    packed_ids.sort();
    packed_ids.dedup();
    assert_eq!(
        packed_ids.len(),
        packing.len(),
        "打包清单内衣物必须去重"
    );
    assert_no_wear(&app, &member);
}

#[test]
fn travel_shortage_is_reported_with_reasons_when_inventory_cannot_rotate() {
    let d = tempfile::tempdir().unwrap();
    let app = open_app(d.path());
    let member = bootstrap_member(&app);
    let created = seed(
        &app,
        &member,
        &[("唯一上衣", "top"), ("唯一长裤", "bottom"), ("唯一鞋", "shoes")],
    );
    let all = ids(&created);
    let plan = app
        .execute(
            "scenario.plan",
            json!({
                "member_id":member,
                "operation_key":"travel-tight",
                "timezone":"UTC",
                "kind":"travel",
                "start_date":"2026-10-10",
                "days":3
            }),
        )
        .unwrap();
    // 库存不足以轮换：必须如实说明，而不是安静地重复或虚构
    let reasons = plan["shortage_reasons"].as_array().unwrap();
    assert!(
        reasons
            .iter()
            .any(|reason| reason.as_str().unwrap_or("").contains("库存不足")),
        "库存不足以跨天轮换须给出短缺原因：{reasons:?}"
    );
    let d0 = day_clothing(&plan, 0);
    let d1 = day_clothing(&plan, 1);
    assert!(
        all.iter().all(|id| d0.contains(id) || d1.contains(id) || day_clothing(&plan, 2).contains(id)),
        "仅使用真实库存"
    );
    assert_no_wear(&app, &member);
}

// ---------------------------------------------------------------------------
// 查看/修改/保存：更新、乐观锁、固定天、删除
// ---------------------------------------------------------------------------

#[test]
fn plans_can_be_viewed_updated_with_optimistic_lock_and_deleted() {
    let d = tempfile::tempdir().unwrap();
    let app = open_app(d.path());
    let member = bootstrap_member(&app);
    let created = seed(
        &app,
        &member,
        &[
            ("礼服A", "dress"),
            ("礼服B", "one_piece"),
            ("高跟鞋", "shoes"),
        ],
    );
    let dress_a = created[0]["id"].as_str().unwrap();
    let dress_b = created[1]["id"].as_str().unwrap();
    let plan = app
        .execute(
            "scenario.plan",
            json!({"member_id":member,"operation_key":"update-1","timezone":"UTC","kind":"event",
                   "occasion":"wedding","start_date":"2026-10-10","fixed_ids":[dress_a]}),
        )
        .unwrap();
    assert!(day_clothing(&plan, 0).contains(&dress_a.to_string()));

    // 排除固定外单品 → 方案换用另一条连衣裙
    let updated = app
        .execute(
            "scenario.update",
            json!({"member_id":member,"id":plan["id"],"expected_version":1,
                   "excluded_ids":[dress_a]}),
        )
        .expect("修改计划应重新生成并保存");
    assert_eq!(updated["version"], 2, "修改后版本递增");
    let day0 = day_clothing(&updated, 0);
    assert!(
        !day0.contains(&dress_a.to_string()),
        "排除后原固定单品不再出现"
    );
    assert!(
        day0.contains(&dress_b.to_string()),
        "重新生成应换用仍可用的连衣裙"
    );
    let got = app
        .execute("scenario.get", json!({"member_id":member,"id":plan["id"]}))
        .unwrap();
    assert_eq!(got["version"], 2, "get 返回最新版本");

    // 乐观锁：过期 expected_version 拒绝
    let stale = app.execute(
        "scenario.update",
        json!({"member_id":member,"id":plan["id"],"expected_version":1,
               "excluded_ids":[dress_b]}),
    );
    assert_eq!(stale.unwrap_err().code, "version_conflict");

    // 保存的模板：把当天方案存为搭配模板不记穿着
    let outfit = app
        .execute(
            "outfit.create",
            json!({"member_id":member,"name":"婚礼穿搭模板",
                   "items":updated["days"][0]["items"].as_array().unwrap().iter()
                       .map(|item|json!({"clothing_id":item["clothing_id"],"role":item["role"],"layer":item["layer"]})).collect::<Vec<_>>()}),
        )
        .unwrap();
    assert_eq!(outfit["status"], "active");
    assert_no_wear(&app, &member);

    // 删除：软删除后 list 默认过滤
    let removed = app
        .execute(
            "scenario.delete",
            json!({"member_id":member,"id":plan["id"]}),
        )
        .unwrap();
    assert_eq!(removed["status"], "deleted");
    let listed = app
        .execute("scenario.list", json!({"member_id":member}))
        .unwrap();
    assert!(
        !listed["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["id"] == plan["id"]),
        "默认列表不显示已删除计划"
    );
}

// ---------------------------------------------------------------------------
// 天气：可选参考，失败降级不阻断
// ---------------------------------------------------------------------------

#[test]
fn weather_is_optional_reference_and_failure_degrades_to_manual_notes() {
    let d = tempfile::tempdir().unwrap();
    let app = open_app(d.path());
    let member = bootstrap_member(&app);
    let created = seed(
        &app,
        &member,
        &[("连衣裙", "dress"), ("大衣", "outerwear"), ("皮鞋", "shoes")],
    );
    // automatic 但未配置天气 → 不可用但计划照常完成（不阻断）
    let plan = app
        .execute(
            "scenario.plan",
            json!({"member_id":member,"operation_key":"weather-auto","timezone":"UTC","kind":"event",
                   "occasion":"wedding","start_date":"2026-10-10",
                   "weather":{"mode":"automatic"},
                   "notes":"婚礼在室内，按室温准备"}),
        )
        .unwrap();
    assert_eq!(
        plan["days"].as_array().unwrap().len(),
        1,
        "天气不可用不阻断计划"
    );
    assert_eq!(plan["weather_context"]["source"], "unavailable");
    assert_eq!(
        plan["controls"]["notes"].as_str().unwrap(),
        "婚礼在室内，按室温准备",
        "手工说明随计划保存"
    );
    assert_eq!(
        day_clothing(&plan, 0).len(),
        2,
        "天气不可用时计划照常基于真实库存完成"
    );
    // 手动低温 → 每个完整方案要求外层
    let cold = app
        .execute(
            "scenario.plan",
            json!({"member_id":member,"operation_key":"weather-manual","timezone":"UTC","kind":"event",
                   "occasion":"wedding","start_date":"2026-10-10",
                   "weather":{"mode":"manual","temperature_c":5}}),
        )
        .unwrap();
    let cold_ids = day_clothing(&cold, 0);
    assert!(
        cold_ids.contains(&created[1]["id"].as_str().unwrap().to_string()),
        "手动低温须要求外层"
    );
    assert!(
        cold["days"][0]["reasons"]
            .as_array()
            .unwrap()
            .iter()
            .any(|reason| reason.as_str().unwrap_or("").contains("外层")),
        "外层决策须在方案理由中如实说明"
    );
    assert_no_wear(&app, &member);
}

// ---------------------------------------------------------------------------
// 模型通道：本地脚本化 HTTP oracle（真实模型成功需用户密钥，另行实测）
// ---------------------------------------------------------------------------

/// 一个脚本化的模型回合：正常回复，或接受 POST 后直接断开（结果未知）。
enum ScriptedTurn {
    Reply(Value),
    CloseWithoutResponse,
}

/// 有界本地 chat-completions 夹具：按序回放脚本响应、记录请求文本、
/// 有界接受连接，Drop 时干净停止（RED 阶段也不悬挂线程）。
struct ScriptedModel {
    endpoint: String,
    posts: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    requests: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    worker: Option<std::thread::JoinHandle<()>>,
}
impl ScriptedModel {
    fn start(turns: Vec<ScriptedTurn>) -> Self {
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
                    ScriptedTurn::CloseWithoutResponse => drop(stream),
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
fn ai_strategy_receipts_before_http_and_worker_adopts_validated_plan() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("db");
    let app = Application::open(&path).unwrap();
    let member = bootstrap_member(&app);
    let created = seed(
        &app,
        &member,
        &[("礼服", "dress"), ("高跟鞋", "shoes"), ("手包", "bag")],
    );
    let dress = created[0]["id"].as_str().unwrap().to_string();
    let model = ScriptedModel::start(vec![
        // 回复合法的逐天指派（与规则候选一致的完整方案）
        ScriptedTurn::Reply(json!({"days":[{"index":0,"clothing_ids":[dress, created[1]["id"], created[2]["id"]]}],"explanation":"婚礼正装"})),
    ]);
    let provider = app
        .execute(
            "provider.save",
            json!({"name":"fixture scenario model","endpoint":model.endpoint,"model":"fixture-text","capabilities":["text"],"authentication":"none"}),
        )
        .unwrap();

    // 收据先行：排队回执在任何 HTTP 之前
    let receipt = app
        .execute(
            "scenario.plan",
            json!({"member_id":member,"operation_key":"ai-wedding","timezone":"UTC","kind":"event",
                   "occasion":"wedding","start_date":"2026-10-10","strategy":"ai",
                   "provider_id":provider["id"]}),
        )
        .expect("RED(ticket-23): strategy=ai 尚未实现");
    assert_eq!(receipt["state"], "queued", "回执必须先于任何模型请求入队");
    assert!(receipt["job_id"].is_string());
    assert_eq!(model.post_count(), 0, "排队回执之前不得发生模型请求");

    // 任务持久化且 kind 明确
    let job_id = receipt["job_id"].as_str().unwrap().to_string();
    let durable = app
        .execute("jobs.get", json!({"member_id":member,"id":job_id}))
        .unwrap();
    assert_eq!(durable["kind"], "scenario_plan");

    // worker 驱动：一次模型 POST，模型输出经服务端校验后采纳
    let job = app
        .execute("scenario.worker.run", json!({"member_id":member,"id":job_id}))
        .expect("worker 必须驱动场景任务");
    assert_eq!(job["state"], "succeeded", "worker 结果：{job}");
    assert_eq!(model.post_count(), 1, "恰好一次模型请求");
    assert!(
        model.request(0).contains("/chat/completions"),
        "模型请求走 chat-completions 文本通道"
    );
    assert!(
        model.request(0).contains(&dress),
        "提示词须携带真实库存候选"
    );
    let plan_id = job["result"]["plan_id"].as_str().expect("结果含计划ID");
    let plan = app
        .execute("scenario.get", json!({"member_id":member,"id":plan_id}))
        .unwrap();
    assert_eq!(
        plan["generation"]["source"], "text_model",
        "模型输出通过服务端校验后标记来源"
    );
    assert_eq!(plan["creates_wear"], false);
    assert_eq!(day_clothing(&plan, 0).len(), 3, "采纳模型的合法逐天方案");
    assert_no_wear(&app, &member);

    // 终态重放不再发起任何 POST
    let replay = app
        .execute("scenario.worker.run", json!({"member_id":member,"id":job_id}))
        .unwrap();
    assert_eq!(replay["state"], "succeeded");
    assert_eq!(model.post_count(), 1, "终态重放不重发模型请求");
}

#[test]
fn model_failure_falls_back_to_rules_with_explicit_feedback() {
    let d = tempfile::tempdir().unwrap();
    let app = open_app(d.path());
    let member = bootstrap_member(&app);
    let created = seed(
        &app,
        &member,
        &[("连衣裙", "dress"), ("高跟鞋", "shoes")],
    );
    let fabricated = uuid::Uuid::now_v7().to_string();
    let model = ScriptedModel::start(vec![
        // 三次全部编造不存在的单品：初始 + 两次重试后必须如实降级
        ScriptedTurn::Reply(json!({"days":[{"index":0,"clothing_ids":[fabricated]}]})),
        ScriptedTurn::Reply(json!({"days":[{"index":0,"clothing_ids":[fabricated]}]})),
        ScriptedTurn::Reply(json!({"not":"valid"})),
    ]);
    let provider = app
        .execute(
            "provider.save",
            json!({"name":"fixture invalid model","endpoint":model.endpoint,"model":"fixture-text","capabilities":["text"],"authentication":"none"}),
        )
        .unwrap();
    let receipt = app
        .execute(
            "scenario.plan",
            json!({"member_id":member,"operation_key":"ai-invalid","timezone":"UTC","kind":"event",
                   "occasion":"wedding","start_date":"2026-10-10","strategy":"ai",
                   "provider_id":provider["id"]}),
        )
        .unwrap();
    let job_id = receipt["job_id"].as_str().unwrap().to_string();
    let job = app
        .execute("scenario.worker.run", json!({"member_id":member,"id":job_id}))
        .unwrap();
    assert_eq!(
        job["state"],
        "succeeded",
        "模型失败但规则可用：降级而不是整体失败"
    );
    assert_eq!(
        model.post_count(),
        3,
        "模型失败须有限重试（初始+2）后降级"
    );
    let plan_id = job["result"]["plan_id"].as_str().unwrap().to_string();
    let plan = app
        .execute("scenario.get", json!({"member_id":member,"id":plan_id}))
        .unwrap();
    assert_eq!(
        plan["generation"]["source"], "rule_fallback",
        "降级计划必须如实标记来源"
    );
    let codes = plan["generation"]["failure_codes"]
        .as_array()
        .expect("失败码数组");
    assert!(
        codes.iter().any(|code| code == "model_invalid_output"),
        "失败码须包含 model_invalid_output：{codes:?}"
    );
    let feedback = plan["generation"]["model_feedback"]
        .as_str()
        .expect("模型失败必须有明确反馈文字");
    assert!(
        feedback.contains("模型") && feedback.contains("规则"),
        "反馈须说明模型失败并已用本地规则：{feedback}"
    );
    assert_eq!(plan["creates_wear"], false);
    assert!(
        day_clothing(&plan, 0)
            .contains(&created[0]["id"].as_str().unwrap().to_string()),
        "降级计划使用真实规则结果"
    );
    assert_no_wear(&app, &member);
}

#[test]
fn unknown_model_outcome_waits_for_input_and_never_blind_resends() {
    let d = tempfile::tempdir().unwrap();
    let app = open_app(d.path());
    let member = bootstrap_member(&app);
    seed(&app, &member, &[("连衣裙", "dress"), ("高跟鞋", "shoes")]);
    let model = ScriptedModel::start(vec![ScriptedTurn::CloseWithoutResponse]);
    let provider = app
        .execute(
            "provider.save",
            json!({"name":"fixture silent model","endpoint":model.endpoint,"model":"fixture-text","capabilities":["text"],"authentication":"none"}),
        )
        .unwrap();
    let receipt = app
        .execute(
            "scenario.plan",
            json!({"member_id":member,"operation_key":"ai-unknown","timezone":"UTC","kind":"event",
                   "occasion":"wedding","start_date":"2026-10-10","strategy":"ai",
                   "provider_id":provider["id"]}),
        )
        .unwrap();
    let job_id = receipt["job_id"].as_str().unwrap().to_string();
    let job = app
        .execute("scenario.worker.run", json!({"member_id":member,"id":job_id}))
        .unwrap();
    assert_eq!(
        job["state"], "waiting_input",
        "提交结果未知进入等待核对，不盲重发"
    );
    assert_eq!(
        job["error"]["code"], "provider_outcome_unknown",
        "结果未知有明确错误反馈"
    );
    // 显式重试被拒绝：提交账本阻止盲重发
    let retry = app.execute("jobs.retry", json!({"member_id":member,"id":job_id}));
    assert_eq!(
        retry.unwrap_err().code,
        "submission_already_sent",
        "已提交任务不能通过 jobs.retry 盲重发"
    );
    let replay = app
        .execute("scenario.worker.run", json!({"member_id":member,"id":job_id}))
        .unwrap();
    assert_eq!(replay["state"], "waiting_input");
    assert_eq!(model.post_count(), 1, "未知结果不重发请求");
    assert_no_wear(&app, &member);
}

// ---------------------------------------------------------------------------
// 入口契约：operations 注册源自动暴露（CLI/MCP/GUI 共用）
// ---------------------------------------------------------------------------

#[test]
fn scenario_operations_are_registered_with_schemas_and_scope() {
    let descriptors = wardrobe_core::operations();
    let names: Vec<&str> = descriptors
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|d| d["name"].as_str())
        .collect();
    for expected in [
        "scenario.plan",
        "scenario.get",
        "scenario.list",
        "scenario.update",
        "scenario.delete",
        "scenario.worker.run",
    ] {
        assert!(
            names.contains(&expected),
            "RED(ticket-23): {expected} 必须经共享注册源暴露"
        );
    }
    let plan = descriptors
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["name"] == json!("scenario.plan"))
        .unwrap();
    assert!(
        plan["inputSchema"]["properties"]["kind"].is_object(),
        "scenario.plan 须声明闭合输入 schema"
    );
    assert_eq!(
        plan["scope"]["member"]["grant"], "per-member_id",
        "scope 元数据由注册源同源派生"
    );
    let registry_dir = tempfile::tempdir().unwrap();
    let app = open_app(registry_dir.path());
    let member = bootstrap_member(&app);
    // 输入字段闭合校验
    let unknown = app.execute(
        "scenario.plan",
        json!({"member_id":member,"operation_key":"x","timezone":"UTC","kind":"event","bogus":1}),
    );
    assert_eq!(unknown.unwrap_err().code, "invalid_input");
}
