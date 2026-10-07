//! Schema23 historical compatibility: public Application upgrade and portable
//! backup/restore seams only. Fixture archives are derived from a real backup of
//! a live application, then reshaped into the known historical variants; the
//! tests never write to a live product database as an oracle. Restored state is
//! asserted through public APIs and exported (public) backup artifacts.
use rusqlite::OptionalExtension;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use wardrobe_core::{Application, DATABASE_SCHEMA_VERSION};

trait WaitsApplication {
    fn wait(&self, operation: &str, input: Value) -> Result<Value, wardrobe_core::ApiError>;
}
impl WaitsApplication for Application {
    fn wait(&self, operation: &str, input: Value) -> Result<Value, wardrobe_core::ApiError> {
        let submitted = self.execute(operation, input)?;
        if let Some(id) = submitted["job_id"].as_str() {
            let job = self.execute("maintenance.worker.run", json!({"id":id}))?;
            if job["state"] == "succeeded" {
                Ok(job["result"].clone())
            } else {
                Err(wardrobe_core::ApiError {
                    code: job["error"]["code"].as_str().unwrap_or("job_cancelled").into(),
                    message: job["error"]["message"].as_str().unwrap_or("任务未完成").into(),
                })
            }
        } else {
            Ok(submitted)
        }
    }
}

// Historical versioned families. The strict legacy shapes (published schema21
// era) carried a composite outfit_versions key and per-root revision
// uniqueness; the sync22 era relaxed all four so lawful branch records survive.
const STRICT_OUTFIT_VERSIONS_DDL: &str = "CREATE TABLE outfit_versions(outfit_id TEXT NOT NULL REFERENCES outfits(id),version INTEGER NOT NULL,document TEXT NOT NULL,PRIMARY KEY(outfit_id,version))";
const RELAXED_OUTFIT_VERSIONS_DDL: &str = "CREATE TABLE outfit_versions(outfit_id TEXT NOT NULL REFERENCES outfits(id),version INTEGER NOT NULL,document TEXT NOT NULL)";
const BRANCH_WEAR_EVENT_ID: &str = "b25e2a9c-8f4d-4e2a-9b1f-3c5d7e9f0a1b";
const BRANCH_CARE_EVENT_ID: &str = "c37f3b0d-9a5e-4f3b-8c2a-4d6e8f0a1b2c";
const BRANCH_FEEDBACK_EVENT_ID: &str = "d48a4c1e-0b6f-4a4c-9d3b-5e7f9a0b1c3d";
const ONE_PIXEL_PNG: &[u8] = &[
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0,
    0, 0, 31, 21, 196, 137, 0, 0, 0, 11, 73, 68, 65, 84, 120, 156, 99, 248, 15, 4, 0, 9, 251, 3,
    253, 251, 94, 107, 43, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
];

fn event_ddl(root: &str, strict: bool) -> String {
    let unique = if strict {
        format!(",UNIQUE({root},revision)")
    } else {
        String::new()
    };
    format!(
        "CREATE TABLE {{table}}(id TEXT PRIMARY KEY,member_id TEXT NOT NULL REFERENCES members(id),{root} TEXT NOT NULL,revision INTEGER NOT NULL,document TEXT NOT NULL{unique})"
    )
}

fn family(table: &str) -> (&'static str, &'static str) {
    match table {
        "wear_events" => ("wear_asset_refs", "root_event_id"),
        "care_events" => ("care_asset_refs", "root_event_id"),
        "feedback_events" => ("feedback_asset_refs", "root_id"),
        _ => ("", ""),
    }
}

fn carries_strict_shapes(sql: &str) -> bool {
    sql.contains("UNIQUE(root_event_id,revision)")
        || sql.contains("PRIMARY KEY(outfit_id,version)")
        || sql.contains("UNIQUE(root_id,revision)")
}

// Rebuilds one versioned family fixture artifact into the requested shape
// (transactionally, with asset-ref children reattached). Skipped when the
// family already carries the requested shape.
fn rebuild_family(db: &std::path::Path, table: &str, strict: bool) {
    let conn = rusqlite::Connection::open(db).unwrap();
    let sql: Option<String> = conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type='table' AND name=?1",
            [table],
            |r| r.get(0),
        )
        .unwrap();
    let Some(sql) = sql else { return };
    if carries_strict_shapes(&sql) == strict {
        return;
    }
    let (refs, root) = family(table);
    conn.execute_batch("PRAGMA foreign_keys=OFF;BEGIN;").unwrap();
    if !refs.is_empty() {
        conn.execute_batch(&format!("ALTER TABLE {refs} RENAME TO {refs}_legacy"))
            .unwrap();
    }
    conn.execute_batch(&format!("ALTER TABLE {table} RENAME TO {table}_legacy"))
        .unwrap();
    let ddl = if table == "outfit_versions" {
        if strict {
            STRICT_OUTFIT_VERSIONS_DDL.to_string()
        } else {
            RELAXED_OUTFIT_VERSIONS_DDL.to_string()
        }
    } else {
        event_ddl(root, strict).replace("{table}", table)
    };
    conn.execute_batch(&ddl).unwrap();
    conn.execute_batch(&format!(
        "INSERT INTO {table} SELECT * FROM {table}_legacy"
    ))
    .unwrap();
    if !refs.is_empty() {
        conn.execute_batch(&format!("CREATE TABLE {refs}(event_id TEXT NOT NULL REFERENCES {table}(id),asset_id TEXT NOT NULL,member_id TEXT NOT NULL,PRIMARY KEY(event_id,asset_id),FOREIGN KEY(asset_id,member_id) REFERENCES assets(id,member_id));INSERT INTO {refs} SELECT * FROM {refs}_legacy;DROP TABLE {refs}_legacy"))
            .unwrap();
    }
    conn.execute_batch(&format!("DROP TABLE {table}_legacy"))
        .unwrap();
    if table != "outfit_versions" {
        conn.execute_batch(&format!(
            "CREATE INDEX IF NOT EXISTS {table}_member_root ON {table}(member_id,{root},revision);"
        ))
        .unwrap();
    }
    conn.execute_batch("COMMIT;").unwrap();
}

fn reshape_all_families(db: &std::path::Path, strict: bool) {
    for table in [
        "wear_events",
        "care_events",
        "feedback_events",
        "outfit_versions",
    ] {
        rebuild_family(db, table, strict);
    }
}

// Reshapes an exported facts.sqlite fixture artifact into the fully relaxed
// sync22 shape across all four versioned families and adds one lawful
// same-root/revision branch row per family. Branch documents clone the
// complete original documents and change only branch identity facts.
fn relax_to_sync22_shape_and_add_branches(db: &std::path::Path, stamp: u32) {
    reshape_all_families(db, false);
    let conn = rusqlite::Connection::open(db).unwrap();
    conn.execute_batch("PRAGMA foreign_keys=OFF;BEGIN;").unwrap();
    let outfit_rows: Vec<(String, i64, String)> = conn
        .prepare("SELECT outfit_id,version,document FROM outfit_versions")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    for (table, id_column, branch_id) in [
        ("wear_events", "root_event_id", BRANCH_WEAR_EVENT_ID),
        ("care_events", "root_event_id", BRANCH_CARE_EVENT_ID),
        ("feedback_events", "root_id", BRANCH_FEEDBACK_EVENT_ID),
    ] {
        let query = format!(
            "SELECT id,member_id,{id_column},revision,document FROM {table} ORDER BY id LIMIT 1"
        );
        let row: Option<(String, String, String, i64, String)> = conn
            .prepare(&query)
            .unwrap()
            .query_row([], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                ))
            })
            .optional()
            .unwrap();
        if let Some((original_id, member_id, root, revision, document)) = row {
            let mut doc: Value = serde_json::from_str(&document).unwrap();
            doc["id"] = json!(branch_id);
            doc["branch"] = json!("sync-beta");
            conn.execute(
                &format!("INSERT INTO {table} VALUES(?1,?2,?3,?4,?5)"),
                rusqlite::params![branch_id, member_id, root, revision, doc.to_string()],
            )
            .unwrap();
            let refs_table = match table {
                "wear_events" => "wear_asset_refs",
                "care_events" => "care_asset_refs",
                _ => "feedback_asset_refs",
            };
            conn.execute(
                &format!(
                    "INSERT INTO {refs_table}(event_id,asset_id,member_id) SELECT ?1,asset_id,member_id FROM {refs_table} WHERE event_id=?2"
                ),
                rusqlite::params![branch_id, original_id],
            )
            .unwrap();
        }
    }
    if let Some((outfit_id, version, document)) = outfit_rows.first() {
        let mut doc: Value = serde_json::from_str(document).unwrap();
        doc["branch"] = json!("sync-beta");
        conn.execute(
            "INSERT INTO outfit_versions VALUES(?1,?2,?3)",
            rusqlite::params![outfit_id, version, doc.to_string()],
        )
        .unwrap();
    }
    conn.execute_batch("COMMIT;").unwrap();
    conn.pragma_update(None, "user_version", stamp).unwrap();
}

fn drop_tables(db: &std::path::Path, tables: &[&str]) {
    let conn = rusqlite::Connection::open(db).unwrap();
    conn.execute_batch("PRAGMA foreign_keys=OFF;").unwrap();
    for table in tables {
        conn.execute(&format!("DROP TABLE IF EXISTS \"{table}\""), [])
            .unwrap();
    }
}

struct Ready {
    app: Application,
    _dir: tempfile::TempDir,
    _archive_parent: tempfile::TempDir,
    garment_id: String,
    asset_id: String,
    member_id: String,
    plan: Value,
    outfit_id: String,
    category_name: String,
    wear_id: String,
    wear_root_id: String,
    care_id: String,
    care_root_id: String,
    feedback_id: String,
    feedback_root_id: String,
    base: std::path::PathBuf,
}

fn setup() -> Ready {
    let dir = tempfile::tempdir().unwrap();
    let archive_parent = tempfile::tempdir().unwrap();
    let app = Application::open(&dir.path().join("facts.sqlite")).unwrap();
    app.execute("space.default", json!({})).unwrap();
    let member_id: String = app
        .execute("member.list", json!({}))
        .unwrap()["items"][0]["member_id"]
        .as_str()
        .unwrap()
        .into();
    let garment = app
        .wait(
            "clothing.create",
            json!({"member_id":"default","name":"兼容衬衫","category":"top"}),
        )
        .unwrap();
    let image = dir.path().join("fixture-image.png");
    std::fs::write(&image, ONE_PIXEL_PNG).unwrap();
    let asset = app
        .execute(
            "assets.import",
            json!({"member_id":"default","path":image}),
        )
        .unwrap();
    app.execute(
        "assets.adopt",
        json!({"member_id":"default","asset_id":asset["id"],"clothing_id":garment["id"]}),
    )
    .unwrap();
    let outfit = app
        .execute(
            "outfit.create",
            json!({"member_id":"default","name":"分支搭配","items":[{"clothing_id":garment["id"],"role":"上装","layer":0}]}),
        )
        .unwrap();
    let wear = app
        .wait(
            "wear.record",
            json!({"member_id":"default","clothing_ids":[garment["id"]],"local_date":"2020-01-01","timezone":"UTC","operation_key":"wear-schema"}),
        )
        .unwrap();
    let care = app
        .wait(
            "care.record",
            json!({"member_id":"default","clothing_id":garment["id"],"action":"wash","local_date":"2020-01-02","timezone":"UTC","operation_key":"care-schema"}),
        )
        .unwrap();
    let intent = app
        .wait(
            "recommendation.generate",
            json!({"member_id":"default","operation_key":"schema-intent","limit":1,"timezone":"UTC"}),
        )
        .unwrap();
    let feedback = app
        .wait(
            "feedback.record",
            json!({"member_id":"default","operation_key":"feedback-schema","intent_id":intent["id"],"proposal_index":0,"reaction":"like"}),
        )
        .unwrap();
    let device = app.execute("schedule.device", json!({})).unwrap();
    let plan = app
        .execute(
            "schedule.create",
            json!({"member_id":"default","name":"历史护理","kind":"care","local_time":"21:30","timezone":"Asia/Shanghai","weekdays":[1,2,3,4,5,6,7],"execution_device_id":device["id"],"occasion":"daily","target_day_offset":0,"enabled":true}),
        )
        .unwrap();
    app.execute("category.create", json!({"member_id":"default","name":"出行的衣"}))
        .unwrap();
    let base = archive_parent.path().join("base23");
    app.wait(
        "backup.create",
        json!({"directory":base,"include_originals":false}),
    )
    .unwrap();
    Ready {
        app,
        _dir: dir,
        _archive_parent: archive_parent,
        garment_id: garment["id"].as_str().unwrap().into(),
        asset_id: asset["id"].as_str().unwrap().into(),
        member_id,
        plan,
        outfit_id: outfit["id"].as_str().unwrap().into(),
        category_name: "出行的衣".into(),
        wear_id: wear["id"].as_str().unwrap().into(),
        wear_root_id: wear["root_event_id"].as_str().unwrap().into(),
        care_id: care["id"].as_str().unwrap().into(),
        care_root_id: care["root_event_id"].as_str().unwrap().into(),
        feedback_id: feedback["id"].as_str().unwrap().into(),
        feedback_root_id: feedback["root_id"].as_str().unwrap().into(),
        base,
    }
}

fn copy_archive(from: &std::path::Path, to: &std::path::Path) {
    fn walk(source: &std::path::Path, target: &std::path::Path) {
        std::fs::create_dir_all(target).unwrap();
        for entry in std::fs::read_dir(source).unwrap() {
            let entry = entry.unwrap();
            let destination = target.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                walk(&entry.path(), &destination);
            } else {
                std::fs::copy(entry.path(), destination).unwrap();
            }
        }
    }
    walk(from, to);
}

// Stamps the fixture facts user_version first (closing the connection so the
// main file is checkpointed), then rewrites database_version and recomputes the
// facts.sqlite integrity entry so the manifest stays internally consistent
// after fixture mutations.
fn stamp_archive(directory: &std::path::Path, version: u64) {
    let facts_path = directory.join("facts.sqlite");
    {
        let conn = rusqlite::Connection::open(&facts_path).unwrap();
        conn.pragma_update(None, "user_version", version).unwrap();
    }
    let facts = std::fs::read(&facts_path).unwrap();
    let hash: String = Sha256::digest(&facts)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let bytes = facts.len() as u64;
    let path = directory.join("manifest.json");
    let mut manifest: Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    manifest["database_version"] = json!(version);
    for entry in manifest["files"].as_array_mut().unwrap() {
        if entry["path"] == "facts.sqlite" {
            entry["sha256"] = json!(hash);
            entry["bytes"] = json!(bytes);
        }
    }
    std::fs::write(&path, manifest.to_string()).unwrap();
}

fn media_checksums(directory: &std::path::Path) -> Vec<(String, u64)> {
    let manifest: Value =
        serde_json::from_str(&std::fs::read_to_string(directory.join("manifest.json")).unwrap())
            .unwrap();
    let mut entries: Vec<(String, u64)> = manifest["files"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|entry| entry["path"].as_str().unwrap_or("").starts_with("media/"))
        .map(|entry| {
            (
                entry["sha256"].as_str().unwrap().to_string(),
                entry["bytes"].as_u64().unwrap(),
            )
        })
        .collect();
    entries.sort();
    entries
}

fn user_version(db: &std::path::Path) -> u32 {
    let conn =
        rusqlite::Connection::open_with_flags(db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    conn.query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap()
}

fn exported_documents(
    db: &rusqlite::Connection,
    query: &str,
    param: &str,
) -> Vec<Value> {
    db.prepare(query)
        .unwrap()
        .query_map([param], |r| {
            Ok(serde_json::from_str::<Value>(&r.get::<_, String>(0)?).unwrap())
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
}

fn assert_branch_pair(original: Value, branch: Value, branch_id: &str, label: &str) {
    let mut normalized = branch.clone();
    if let Some(map) = normalized.as_object_mut() {
        map.remove("branch");
    }
    normalized["id"] = original["id"].clone();
    assert_eq!(
        normalized, original,
        "{label} 分支文档必须是完整原始事实的分支副本"
    );
    assert_eq!(branch["id"], json!(branch_id), "{label} 分支身份须保留");
    assert_eq!(branch["branch"], "sync-beta", "{label} 分支标记须保留");
}

// Asserts the exported archive carries the exact (event_id, asset_id,
// member_id) reference pairs for both the original and the branch row of a
// versioned family.
fn assert_exact_refs(
    db: &rusqlite::Connection,
    table: &str,
    expected: &[(&str, &str, &str)],
    label: &str,
) {
    let mut actual: Vec<(String, String, String)> = db
        .prepare(&format!(
            "SELECT event_id,asset_id,member_id FROM {table} ORDER BY event_id"
        ))
        .unwrap()
        .query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get(1)?, r.get(2)?))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    actual.sort();
    let mut expected: Vec<(String, String, String)> = expected
        .iter()
        .map(|(event, asset, member)| {
            (
                event.to_string(),
                asset.to_string(),
                member.to_string(),
            )
        })
        .collect();
    expected.sort();
    assert_eq!(actual, expected, "{label}资产引用须逐行精确保留");
}

#[test]
fn combined_schema23_is_the_portable_baseline() {
    assert_eq!(DATABASE_SCHEMA_VERSION, 23);
    let ready = setup();
    let preview = ready
        .app
        .wait("backup.preview", json!({"directory":ready.base}))
        .unwrap();
    assert_eq!(preview["valid"], true);
    assert_eq!(preview["manifest"]["database_version"], 23);
    assert_eq!(preview["incoming"]["assets"], 1);
    assert_eq!(media_checksums(&ready.base).len(), 2, "展示图与缩略图须入备份");
}

#[test]
fn published_category_only_21_archive_previews_and_restores() {
    let ready = setup();
    let variant = ready._archive_parent.path().join("category-only-21");
    copy_archive(&ready.base, &variant);
    let facts = variant.join("facts.sqlite");
    reshape_all_families(&facts, true);
    drop_tables(&facts, &["schedule_executions", "schedule_plans"]);
    stamp_archive(&variant, 21);
    let preview = ready
        .app
        .wait("backup.preview", json!({"directory":variant}))
        .unwrap();
    assert_eq!(preview["valid"], true);
    assert_eq!(preview["incoming"]["schedule_plans"], 0);
    assert_eq!(preview["incoming"]["assets"], 1);
    let target = tempfile::tempdir().unwrap();
    let restored = Application::open(&target.path().join("facts.sqlite")).unwrap();
    restored
        .wait(
            "backup.restore",
            json!({"directory":variant,"confirm_overwrite":true}),
        )
        .unwrap();
    let listed = restored
        .execute("clothing.list", json!({"member_id":"default"}))
        .unwrap();
    assert_eq!(
        listed["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["id"] == ready.garment_id),
        true
    );
    let categories = restored
        .execute("category.list", json!({"member_id":"default"}))
        .unwrap();
    assert_eq!(
        categories.to_string().contains(&ready.category_name),
        true,
        "分类注册表须在恢复后保留"
    );
    let asset = restored
        .execute(
            "assets.get",
            json!({"member_id":"default","id":ready.asset_id}),
        )
        .unwrap();
    assert_eq!(asset["id"], json!(ready.asset_id));
    let history = restored
        .execute(
            "wear.history",
            json!({"member_id":"default","root_event_id":ready.wear_root_id}),
        )
        .unwrap();
    assert_eq!(
        history["items"].as_array().unwrap().len(),
        1,
        "严格21恢复后穿着历史须完整"
    );
}

#[test]
fn schedule_only_21_archive_previews_and_restores() {
    let ready = setup();
    let variant = ready._archive_parent.path().join("schedule-only-21");
    copy_archive(&ready.base, &variant);
    let facts = variant.join("facts.sqlite");
    reshape_all_families(&facts, true);
    drop_tables(&facts, &["wardrobe_categories"]);
    stamp_archive(&variant, 21);
    let preview = ready
        .app
        .wait("backup.preview", json!({"directory":variant}))
        .unwrap();
    assert_eq!(preview["valid"], true);
    let target = tempfile::tempdir().unwrap();
    let restored = Application::open(&target.path().join("facts.sqlite")).unwrap();
    restored
        .wait(
            "backup.restore",
            json!({"directory":variant,"confirm_overwrite":true}),
        )
        .unwrap();
    let plans = restored
        .execute("schedule.list", json!({"member_id":"default"}))
        .unwrap();
    assert_eq!(plans["plans"][0]["id"], ready.plan["id"]);
    let history = restored
        .execute(
            "care.history",
            json!({"member_id":"default","root_event_id":ready.care_root_id}),
        )
        .unwrap();
    assert_eq!(
        history["items"].as_array().unwrap().len(),
        1,
        "严格21恢复后护理历史须完整"
    );
}

#[test]
fn sync22_archives_import_with_branch_records_preserved() {
    for name in ["sync22-historical-28", "sync22-post-category-29"] {
        let ready = setup();
        let variant = ready._archive_parent.path().join(name);
        copy_archive(&ready.base, &variant);
        let facts = variant.join("facts.sqlite");
        relax_to_sync22_shape_and_add_branches(&facts, 22);
        if name == "sync22-historical-28" {
            drop_tables(&facts, &["wardrobe_categories"]);
        }
        drop_tables(&facts, &["schedule_executions", "schedule_plans"]);
        stamp_archive(&variant, 22);
        let preview = ready
            .app
            .wait("backup.preview", json!({"directory":variant}))
            .unwrap();
        assert_eq!(preview["valid"], true, "{name} 预览须有效");
        assert_eq!(
            preview["incoming"]["outfit_versions"], 2,
            "{name} 须带两个版本分支"
        );
        for table in ["wear_events", "care_events", "feedback_events"] {
            assert_eq!(
                preview["incoming"][table], 2,
                "{name} {table} 须带两个同根分支"
            );
        }
        assert_eq!(
            preview["incoming"]["schedule_plans"], 0,
            "{name} 不得包含计划表"
        );
        if name == "sync22-historical-28" {
            assert_eq!(preview["incoming"]["wardrobe_categories"], 0);
        } else {
            assert_eq!(preview["incoming"]["wardrobe_categories"], 1);
        }
        let target = tempfile::tempdir().unwrap();
        let restored = Application::open(&target.path().join("facts.sqlite")).unwrap();
        restored
            .wait(
                "backup.restore",
                json!({"directory":variant,"confirm_overwrite":true}),
            )
            .unwrap();
        // Public API: every branch-capable family exposes both lawful rows.
        let version_doc = restored
            .execute(
                "outfit.version",
                json!({"member_id":"default","id":ready.outfit_id,"version":1}),
            )
            .unwrap();
        assert_eq!(version_doc["id"], ready.outfit_id);
        let wear_history = restored
            .execute(
                "wear.history",
                json!({"member_id":"default","root_event_id":ready.wear_root_id}),
            )
            .unwrap();
        assert_eq!(
            wear_history["items"].as_array().unwrap().len(),
            2,
            "{name} 穿着历史须同时返回两个同根分支"
        );
        let care_history = restored
            .execute(
                "care.history",
                json!({"member_id":"default","root_event_id":ready.care_root_id}),
            )
            .unwrap();
        assert_eq!(
            care_history["items"].as_array().unwrap().len(),
            2,
            "{name} 护理历史须同时返回两个同根分支"
        );
        let feedback_history = restored
            .execute(
                "feedback.history",
                json!({"member_id":"default","root_id":ready.feedback_root_id,"limit":10}),
            )
            .unwrap();
        assert_eq!(
            feedback_history["items"].as_array().unwrap().len(),
            2,
            "{name} 反馈历史须同时返回两个同根分支"
        );
        // Public artifact seam: re-export the restored facts and inspect the
        // exported archive (never the live private database) for the exact
        // complete branch documents, asset references, and media checksums.
        let reexport = target.path().join("reexport");
        restored
            .wait(
                "backup.create",
                json!({"directory":reexport,"include_originals":false}),
            )
            .unwrap();
        let exported = rusqlite::Connection::open_with_flags(
            reexport.join("facts.sqlite"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        let mut outfit_docs = exported_documents(
            &exported,
            "SELECT document FROM outfit_versions WHERE outfit_id=?1 AND version=1",
            &ready.outfit_id,
        );
        outfit_docs.sort_by_key(|doc| doc.get("branch").is_none());
        assert_eq!(outfit_docs.len(), 2, "{name} 导出须保留两个搭配分支");
        assert_branch_pair(
            outfit_docs[1].clone(),
            outfit_docs[0].clone(),
            ready.outfit_id.as_str(),
            &format!("{name} 搭配"),
        );
        let wear_docs = exported_documents(
            &exported,
            "SELECT document FROM wear_events WHERE root_event_id=?1 ORDER BY id",
            &ready.wear_root_id,
        );
        assert_eq!(wear_docs.len(), 2, "{name} 导出须保留两个穿着分支");
        let (wear_original, wear_branch) = if wear_docs[0]["id"] == json!(BRANCH_WEAR_EVENT_ID) {
            (wear_docs[1].clone(), wear_docs[0].clone())
        } else {
            (wear_docs[0].clone(), wear_docs[1].clone())
        };
        assert_branch_pair(
            wear_original,
            wear_branch,
            BRANCH_WEAR_EVENT_ID,
            &format!("{name} 穿着"),
        );
        let care_docs = exported_documents(
            &exported,
            "SELECT document FROM care_events WHERE root_event_id=?1 ORDER BY id",
            &ready.care_root_id,
        );
        assert_eq!(care_docs.len(), 2, "{name} 导出须保留两个护理分支");
        let (care_original, care_branch) = if care_docs[0]["id"] == json!(BRANCH_CARE_EVENT_ID) {
            (care_docs[1].clone(), care_docs[0].clone())
        } else {
            (care_docs[0].clone(), care_docs[1].clone())
        };
        assert_branch_pair(care_original, care_branch, BRANCH_CARE_EVENT_ID, &format!("{name} 护理"));
        let feedback_docs = exported_documents(
            &exported,
            "SELECT document FROM feedback_events WHERE root_id=?1 ORDER BY id",
            &ready.feedback_root_id,
        );
        assert_eq!(feedback_docs.len(), 2, "{name} 导出须保留两个反馈分支");
        let (feedback_original, feedback_branch) =
            if feedback_docs[0]["id"] == json!(BRANCH_FEEDBACK_EVENT_ID) {
                (feedback_docs[1].clone(), feedback_docs[0].clone())
            } else {
                (feedback_docs[0].clone(), feedback_docs[1].clone())
            };
        assert_branch_pair(
            feedback_original,
            feedback_branch,
            BRANCH_FEEDBACK_EVENT_ID,
            &format!("{name} 反馈"),
        );
        for (table, label) in [
            ("wear_asset_refs", "穿着"),
            ("care_asset_refs", "护理"),
            ("feedback_asset_refs", "反馈"),
        ] {
            let branch_id = match table {
                "wear_asset_refs" => BRANCH_WEAR_EVENT_ID,
                "care_asset_refs" => BRANCH_CARE_EVENT_ID,
                _ => BRANCH_FEEDBACK_EVENT_ID,
            };
            let original_id = match table {
                "wear_asset_refs" => ready.wear_id.as_str(),
                "care_asset_refs" => ready.care_id.as_str(),
                _ => ready.feedback_id.as_str(),
            };
            assert_exact_refs(
                &exported,
                table,
                &[
                    (original_id, ready.asset_id.as_str(), &ready.member_id),
                    (branch_id, ready.asset_id.as_str(), &ready.member_id),
                ],
                &format!("{name} {label}"),
            );
        }
        let assets: i64 = exported
            .query_row("SELECT COUNT(*) FROM assets", [], |r| r.get(0))
            .unwrap();
        assert_eq!(assets, 1, "{name} 导出须保留资产记录");
        assert_eq!(
            media_checksums(&variant),
            media_checksums(&reexport),
            "{name} 恢复后导出的媒体校验和须与来源一致"
        );
        let exported_version: u32 = exported
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(exported_version, 23, "{name} 恢复后不得降级");
        let reexport_preview = restored
            .wait("backup.preview", json!({"directory":reexport}))
            .unwrap();
        assert_eq!(reexport_preview["incoming"]["outfit_versions"], 2);
        for table in ["wear_events", "care_events", "feedback_events"] {
            assert_eq!(reexport_preview["incoming"][table], 2);
        }
    }
}

#[test]
fn unknown_or_malformed_archive_shapes_are_rejected() {
    let ready = setup();
    // 28-table baseline stamped 21: no known 21 shape has this table set.
    let wrong_21 = ready._archive_parent.path().join("wrong-21");
    copy_archive(&ready.base, &wrong_21);
    let facts = wrong_21.join("facts.sqlite");
    relax_to_sync22_shape_and_add_branches(&facts, 21);
    drop_tables(&facts, &["wardrobe_categories"]);
    drop_tables(&facts, &["schedule_executions", "schedule_plans"]);
    stamp_archive(&wrong_21, 21);
    assert!(
        ready
            .app
            .wait("backup.preview", json!({"directory":wrong_21}))
            .is_err(),
        "未知21表集须拒绝"
    );
    // Combined 31-table set stamped 22: no known 22 shape carries schedules.
    let wrong_22 = ready._archive_parent.path().join("wrong-22");
    copy_archive(&ready.base, &wrong_22);
    stamp_archive(&wrong_22, 22);
    assert!(
        ready
            .app
            .wait("backup.preview", json!({"directory":wrong_22}))
            .is_err(),
        "未知22表集须拒绝"
    );
    // Arbitrary subset of a known strict shape: category29 minus
    // learning_receipts at 21.
    let subset = ready._archive_parent.path().join("subset-21");
    copy_archive(&ready.base, &subset);
    let facts = subset.join("facts.sqlite");
    reshape_all_families(&facts, true);
    drop_tables(
        &facts,
        &["schedule_executions", "schedule_plans", "learning_receipts"],
    );
    stamp_archive(&subset, 21);
    assert!(
        ready
            .app
            .wait("backup.preview", json!({"directory":subset}))
            .is_err(),
        "任意子集白名单须拒绝"
    );
    // Relaxed versioned shapes stamped 21: schema21 required the strict
    // shapes, so a synthetic relaxed21 archive is rejected.
    let relaxed_21 = ready._archive_parent.path().join("relaxed-21");
    copy_archive(&ready.base, &relaxed_21);
    let facts = relaxed_21.join("facts.sqlite");
    drop_tables(&facts, &["schedule_executions", "schedule_plans"]);
    stamp_archive(&relaxed_21, 21);
    assert!(
        ready
            .app
            .wait("backup.preview", json!({"directory":relaxed_21}))
            .is_err(),
        "宽松21形状须拒绝"
    );
    // Strict versioned shapes stamped 22: schema22 archives are relaxed, so a
    // synthetic strict22 archive is rejected.
    let strict_22 = ready._archive_parent.path().join("strict-22");
    copy_archive(&ready.base, &strict_22);
    let facts = strict_22.join("facts.sqlite");
    reshape_all_families(&facts, true);
    drop_tables(&facts, &["schedule_executions", "schedule_plans"]);
    stamp_archive(&strict_22, 22);
    assert!(
        ready
            .app
            .wait("backup.preview", json!({"directory":strict_22}))
            .is_err(),
        "严格22形状须拒绝"
    );
    // Unknown table in an otherwise valid 23 archive.
    let unknown = ready._archive_parent.path().join("unknown-23");
    copy_archive(&ready.base, &unknown);
    let conn = rusqlite::Connection::open(unknown.join("facts.sqlite")).unwrap();
    conn.execute_batch("CREATE TABLE wardrobe_extra(id TEXT);")
        .unwrap();
    drop(conn);
    stamp_archive(&unknown, 23);
    assert!(
        ready
            .app
            .wait("backup.preview", json!({"directory":unknown}))
            .is_err(),
        "未知表须拒绝"
    );
    // Arbitrary extra unique index in an otherwise valid 23 archive.
    let extra_unique = ready._archive_parent.path().join("extra-unique-23");
    copy_archive(&ready.base, &extra_unique);
    let conn = rusqlite::Connection::open(extra_unique.join("facts.sqlite")).unwrap();
    conn.execute_batch("CREATE UNIQUE INDEX clothing_member_extra ON clothing(member_id);")
        .unwrap();
    drop(conn);
    stamp_archive(&extra_unique, 23);
    assert!(
        ready
            .app
            .wait("backup.preview", json!({"directory":extra_unique}))
            .is_err(),
        "任意额外唯一索引须拒绝"
    );
    // Future version: never downgrade, reject.
    let future = ready._archive_parent.path().join("future-24");
    copy_archive(&ready.base, &future);
    stamp_archive(&future, 24);
    assert!(
        ready
            .app
            .wait("backup.preview", json!({"directory":future}))
            .is_err(),
        "未来版本须拒绝"
    );
}

// Copies the closed application database file as a fixture artifact, reshapes
// it into a historical local variant, and reopens it through the public
// Application seam. user_version is read back read-only after close because the
// migration stamp has no public API; all data assertions go through public
// operations.
fn open_local_variant(
    source_db: &std::path::Path,
    reshape: impl FnOnce(&std::path::Path),
) -> (Application, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("facts.sqlite");
    std::fs::copy(source_db, &db).unwrap();
    reshape(&db);
    let app = Application::open(&db).unwrap();
    (app, dir)
}

#[test]
fn application_open_upgrades_known_historical_local_databases() {
    let ready = setup();
    let live_db = ready._dir.path().join("facts.sqlite");
    drop(ready.app);

    // Both-known-21 shape: full combined tables in the authentic strict shapes
    // stamped 21; the open upgrade must rebuild them losslessly.
    let (app, keep) = open_local_variant(&live_db, |db| {
        reshape_all_families(db, true);
        let conn = rusqlite::Connection::open(db).unwrap();
        conn.pragma_update(None, "user_version", 21).unwrap();
    });
    let plans = app
        .execute("schedule.list", json!({"member_id":"default"}))
        .unwrap();
    assert_eq!(plans["plans"][0]["id"], ready.plan["id"]);
    let wear_history = app
        .execute(
            "wear.history",
            json!({"member_id":"default","root_event_id":ready.wear_root_id}),
        )
        .unwrap();
    assert_eq!(wear_history["items"].as_array().unwrap().len(), 1);
    let care_history = app
        .execute(
            "care.history",
            json!({"member_id":"default","root_event_id":ready.care_root_id}),
        )
        .unwrap();
    assert_eq!(care_history["items"].as_array().unwrap().len(), 1);
    let asset = app
        .execute(
            "assets.get",
            json!({"member_id":"default","id":ready.asset_id}),
        )
        .unwrap();
    assert_eq!(asset["id"], json!(ready.asset_id));
    let feedback_history = app
        .execute(
            "feedback.history",
            json!({"member_id":"default","root_id":ready.feedback_root_id,"limit":10}),
        )
        .unwrap();
    assert_eq!(
        feedback_history["items"].as_array().unwrap().len(),
        1,
        "严格21重建后反馈历史须完整"
    );
    // Public artifact seam for the rebuilt database: exact child references,
    // media checksums, and a valid combined archive.
    let reexport = keep.path().join("reexport");
    app.wait(
        "backup.create",
        json!({"directory":reexport,"include_originals":false}),
    )
    .unwrap();
    let exported = rusqlite::Connection::open_with_flags(
        reexport.join("facts.sqlite"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    for (table, event_id) in [
        ("wear_asset_refs", ready.wear_id.as_str()),
        ("care_asset_refs", ready.care_id.as_str()),
        ("feedback_asset_refs", ready.feedback_id.as_str()),
    ] {
        assert_exact_refs(
            &exported,
            table,
            &[(event_id, ready.asset_id.as_str(), &ready.member_id)],
            "严格21重建",
        );
    }
    assert_eq!(
        media_checksums(&reexport),
        media_checksums(&ready.base),
        "严格21重建后媒体校验和须保持"
    );
    app.wait("backup.preview", json!({"directory":reexport}))
        .unwrap();
    drop(exported);
    drop(app);
    assert_eq!(
        user_version(&keep.path().join("facts.sqlite")),
        23,
        "打开须升级到23"
    );

    // Category-only 21: published authoritative shape without schedules.
    let (app, keep) = open_local_variant(&live_db, |db| {
        reshape_all_families(db, true);
        drop_tables(db, &["schedule_executions", "schedule_plans"]);
        let conn = rusqlite::Connection::open(db).unwrap();
        conn.pragma_update(None, "user_version", 21).unwrap();
    });
    let categories = app
        .execute("category.list", json!({"member_id":"default"}))
        .unwrap();
    assert_eq!(categories.to_string().contains(&ready.category_name), true);
    let device = app.execute("schedule.device", json!({})).unwrap();
    let created = app.execute(
        "schedule.create",
        json!({"member_id":"default","name":"新计划","kind":"care","local_time":"20:00","timezone":"Asia/Shanghai","weekdays":[1],"execution_device_id":device["id"],"occasion":"daily","target_day_offset":0,"enabled":true}),
    );
    assert!(created.is_ok(), "升级后必须能创建计划");
    drop(app);
    assert_eq!(user_version(&keep.path().join("facts.sqlite")), 23);

    // Schedule-only 21: unreleased shape without the category registry.
    let (app, keep) = open_local_variant(&live_db, |db| {
        reshape_all_families(db, true);
        drop_tables(db, &["wardrobe_categories"]);
        let conn = rusqlite::Connection::open(db).unwrap();
        conn.pragma_update(None, "user_version", 21).unwrap();
    });
    let plans = app
        .execute("schedule.list", json!({"member_id":"default"}))
        .unwrap();
    assert_eq!(plans["plans"][0]["id"], ready.plan["id"]);
    assert!(
        app.execute("category.create", json!({"member_id":"default","name":"补充分类"}))
            .is_ok(),
        "升级后分类注册表必须可用"
    );
    drop(app);
    assert_eq!(user_version(&keep.path().join("facts.sqlite")), 23);

    // Sync22 local database: fully relaxed shapes with lawful branches, local
    // transport receipts, no schedules; receipts and branch facts must survive
    // the upgrade and the version must never go down.
    let (app, keep) = open_local_variant(&live_db, |db| {
        drop_tables(db, &["schedule_executions", "schedule_plans"]);
        relax_to_sync22_shape_and_add_branches(db, 22);
        let conn = rusqlite::Connection::open(db).unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS sync_resolution_receipts(member_id TEXT NOT NULL,operation_key TEXT NOT NULL,request TEXT NOT NULL,result TEXT NOT NULL,PRIMARY KEY(member_id,operation_key));INSERT INTO sync_resolution_receipts VALUES('default','upgrade-probe','{\"probe\":true}','{\"probe\":true}');",
        )
        .unwrap();
    });
    let plans = app
        .execute("schedule.list", json!({"member_id":"default"}))
        .unwrap();
    assert_eq!(plans["plans"].as_array().map(|p| p.len()), Some(0));
    let categories = app
        .execute("category.list", json!({"member_id":"default"}))
        .unwrap();
    assert_eq!(
        categories.to_string().contains(&ready.category_name),
        true,
        "sync22升级后分类注册表须可用"
    );
    let wear_history = app
        .execute(
            "wear.history",
            json!({"member_id":"default","root_event_id":ready.wear_root_id}),
        )
        .unwrap();
    assert_eq!(
        wear_history["items"].as_array().unwrap().len(),
        2,
        "sync22升级后穿着分支须保留"
    );
    drop(app);
    let db = keep.path().join("facts.sqlite");
    assert_eq!(
        user_version(&db),
        23,
        "sync22 打开须升级到23且不得降级"
    );
    let conn =
        rusqlite::Connection::open_with_flags(&db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    let receipts: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sync_resolution_receipts WHERE operation_key='upgrade-probe'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(receipts, 1, "sync22 本地回执必须在升级后保留");
}

#[test]
fn fresh_open_is_idempotent_at_schema23() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("facts.sqlite");
    {
        let _app = Application::open(&db).unwrap();
    }
    assert_eq!(user_version(&db), 23);
    {
        let _app = Application::open(&db).unwrap();
    }
    assert_eq!(user_version(&db), 23);
}
