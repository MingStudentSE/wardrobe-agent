//! profile 连接矩阵：真实子进程，隔离配置路径，绝不触碰用户配置或系统 bin。

use serde_json::json;
use std::process::Command;

struct Env {
    config: std::path::PathBuf,
    dir: tempfile::TempDir,
}

impl Env {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        Self {
            config: dir.path().join("cfg").join("profiles.json"),
            dir,
        }
    }
    fn cmd(&self, args: &[&str]) -> (i32, String) {
        let output = Command::new(env!("CARGO_BIN_EXE_wardrobe-cli"))
            .args(args)
            .env("WARDROBE_PROFILES_PATH", &self.config)
            .env_remove("WARDROBE_DATA_DIR")
            .output()
            .unwrap();
        (
            output.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&output.stdout).to_string(),
        )
    }
    fn data_dir(&self, name: &str) -> std::path::PathBuf {
        let d = self.dir.path().join(name);
        std::fs::create_dir_all(&d).unwrap();
        d
    }
}

#[test]
fn profile_lifecycle_isolation_and_fallback_matrix() {
    let env = Env::new();
    let a = env.data_dir("data-a");
    let b = env.data_dir("data-b");

    // 未知profile：exit 2 纯JSON，不创建目录
    let (code, out) = env.cmd(&[
        "--trusted-local",
        "--profile",
        "ghost",
        "space.default",
        "{}",
    ]);
    assert_eq!(code, 2, "{out}");
    let err: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(err["error"]["code"], json!("profile_not_found"), "{out}");

    // add两个隔离profile
    let (code, out) = env.cmd(&["profile.add", "work", "--data-dir", a.to_str().unwrap()]);
    assert_eq!(code, 0, "{out}");
    let (code, out) = env.cmd(&["profile.add", "home", "--data-dir", b.to_str().unwrap()]);
    assert_eq!(code, 0, "{out}");
    // 重复添加拒绝；--force 幂等覆盖
    let (code, out) = env.cmd(&["profile.add", "work", "--data-dir", b.to_str().unwrap()]);
    assert_eq!(code, 2);
    assert!(out.contains("profile_exists"), "{out}");
    let (code, out) = env.cmd(&[
        "profile.add",
        "work",
        "--data-dir",
        a.to_str().unwrap(),
        "--force",
    ]);
    assert_eq!(code, 0, "{out}");
    let (code, out) = env.cmd(&["profile.list"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("data-a") && out.contains("data-b"), "{out}");

    // 隔离：work 与 home 各自 space.default 成员互不可见
    let (code, out) = env.cmd(&[
        "--trusted-local",
        "--profile",
        "work",
        "space.default",
        "{}",
    ]);
    assert_eq!(code, 0, "{out}");
    let member_a: String = serde_json::from_str::<serde_json::Value>(&out).unwrap()["result"]
        ["member_id"]
        .as_str()
        .unwrap()
        .into();
    let (code, out) = env.cmd(&[
        "--trusted-local",
        "--profile",
        "home",
        "space.default",
        "{}",
    ]);
    assert_eq!(code, 0, "{out}");
    let member_b: String = serde_json::from_str::<serde_json::Value>(&out).unwrap()["result"]
        ["member_id"]
        .as_str()
        .unwrap()
        .into();
    assert_ne!(member_a, member_b, "两个数据目录必须隔离");
    let (code, out) = env.cmd(&[
        "--trusted-local",
        "--profile",
        "home",
        "clothing.create",
        &format!("{{\"member_id\":\"{member_b}\",\"name\":\"home专属\",\"category\":\"top\"}}"),
    ]);
    assert_eq!(code, 0, "{out}");
    let (code, out) = env.cmd(&[
        "--trusted-local",
        "--profile",
        "work",
        "clothing.list",
        &format!("{{\"member_id\":\"{member_a}\"}}"),
    ]);
    assert_eq!(code, 0, "{out}");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&out).unwrap()["result"]["items"],
        json!([]),
        "work读不到home的数据"
    );

    // profile.use 设置默认后省略 --profile 即走默认目录
    let (code, out) = env.cmd(&["profile.use", "home"]);
    assert_eq!(code, 0, "{out}");
    let (code, out) = env.cmd(&["--trusted-local", "space.default", "{}"]);
    assert_eq!(code, 0, "{out}");
    let again: String = serde_json::from_str::<serde_json::Value>(&out).unwrap()["result"]
        ["member_id"]
        .as_str()
        .unwrap()
        .into();
    assert_eq!(again, member_b, "默认profile解析到home");

    // --db 与 --profile 互斥
    let (code, out) = env.cmd(&[
        "--db",
        a.join("x.sqlite").to_str().unwrap(),
        "--profile",
        "work",
        "space.default",
        "{}",
    ]);
    assert_eq!(code, 2, "{out}");

    // 非法名称 exit 2
    let (code, out) = env.cmd(&["profile.add", "-bad", "--data-dir", a.to_str().unwrap()]);
    assert_eq!(code, 2, "{out}");

    // 静态入口不受配置影响：即使配置文件损坏
    std::fs::create_dir_all(env.config.parent().unwrap()).unwrap();
    std::fs::write(&env.config, b"{corrupt").unwrap();
    let (code, out) = env.cmd(&["--version"]);
    assert_eq!(code, 0, "{out}");
    let (code, out) = env.cmd(&[
        "--trusted-local",
        "--profile",
        "work",
        "space.default",
        "{}",
    ]);
    assert_eq!(code, 1, "损坏配置=storage_error exit 1");
    assert!(out.contains("storage_error"), "{out}");

    // 修复后 remove：默认指针清除并回落；数据目录保留
    std::fs::remove_file(&env.config).unwrap();
    env.cmd(&["profile.add", "work", "--data-dir", a.to_str().unwrap()]);
    env.cmd(&["profile.use", "work"]);
    let (code, out) = env.cmd(&["profile.remove", "work"]);
    assert_eq!(code, 0, "{out}");
    assert!(a.join("wardrobe.sqlite").exists(), "配置删除不删衣橱数据");
    let (code, out) = env.cmd(&["profile.remove", "work"]);
    assert_eq!(code, 2, "{out}");
    assert!(out.contains("profile_not_found"), "{out}");
}

#[test]
fn mcp_profile_resolution_matches_cli_without_gui() {
    let env = Env::new();
    let a = env.data_dir("mcp-a");
    let (code, out) = env.cmd(&["profile.add", "mcp", "--data-dir", a.to_str().unwrap()]);
    assert_eq!(code, 0, "{out}");
    let (code, out) = env.cmd(&["--trusted-local", "--profile", "mcp", "space.default", "{}"]);
    assert_eq!(code, 0, "{out}");
    let member: String = serde_json::from_str::<serde_json::Value>(&out).unwrap()["result"]
        ["member_id"]
        .as_str()
        .unwrap()
        .into();
    // MCP 用同一 profile 写入，CLI 读回同一持久数据
    let mut child = Command::new(env!("CARGO_BIN_EXE_wardrobe-cli"))
        .args(["--trusted-local", "mcp", "--profile", "mcp"])
        .env("WARDROBE_PROFILES_PATH", &env.config)
        .env_remove("WARDROBE_DATA_DIR")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    use std::io::Write;
    let mut stdin = child.stdin.take().unwrap();
    let init =
        String::from("{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{}}");
    writeln!(stdin, "{init}").unwrap();
    let call = String::from(
        "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/call\",\"params\":{\"name\":\"capabilities\",\"arguments\":{}}}",
    );
    writeln!(stdin, "{call}").unwrap();
    drop(stdin);
    let out = child.wait_with_output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(stdout.contains("\"id\":1"), "MCP initialize应答: {stdout}");
    let (code, out) = env.cmd(&[
        "--trusted-local",
        "--profile",
        "mcp",
        "clothing.list",
        &format!("{{\"member_id\":\"{member}\"}}"),
    ]);
    assert_eq!(code, 0, "{out}");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&out).unwrap()["result"]["items"],
        json!([])
    );
}
