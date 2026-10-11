//! #71 同步范围声明契约：SYNCED_TABLES 与真实触发器一致，SYNC_EXCLUDED_TABLES
//! 中的表不得出现任何 sync_* 触发器。同步范围是显式决策——扩表或缩表必须
//! 先改 sync/mod.rs 的两份清单，再改实现；本测试让声明免于腐烂。
use rusqlite::Connection;
use wardrobe_core::{sync::SYNCED_TABLES, sync::SYNC_EXCLUDED_TABLES, Application};

#[test]
fn declared_sync_scope_matches_real_triggers_and_exclusions() {
    let dir = tempfile::tempdir().unwrap();
    let database = dir.path().join("wardrobe.sqlite");
    let app = Application::open(&database).unwrap();
    drop(app);
    let connection = Connection::open(&database).unwrap();
    let mut statement = connection
        .prepare("SELECT name FROM sqlite_master WHERE type='trigger' AND name LIKE 'sync_%'")
        .unwrap();
    let mut triggers: Vec<String> = statement
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    drop(statement);
    triggers.sort();

    let mut expected: Vec<String> = SYNCED_TABLES
        .iter()
        .flat_map(|table| ["INSERT", "UPDATE"].map(|kind| format!("sync_{table}_{kind}")))
        .collect();
    expected.sort();
    assert_eq!(
        triggers, expected,
        "进同步的表清单必须与真实 sync_* 触发器一一对应"
    );

    for table in SYNC_EXCLUDED_TABLES {
        let prefix = format!("sync_{table}_");
        assert!(
            !triggers.iter().any(|name| name.starts_with(&prefix)),
            "声明为不进同步的表 {table} 不得挂同步触发器"
        );
    }
}
