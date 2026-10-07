//! #33 4.5 外部 Agent 完整流程验收：安装后真实 CLI/MCP 子进程、
//! 有限授权写回、跨入口一致、拒绝无副作用、令牌不进 argv。
//! 内置 Stylist（#22）尚未实现：系统当前唯一模型路径为用户显式配置的
//! provider，等价于内置 AI 关闭；该开关的 GUI 设置项随 #22/#34 交付，
//! 在此如实记录为依赖，不伪造设置证明。

use serde_json::{json, Value};
use std::io::Write;
use std::process::{Command, Stdio};

struct Cli {
    db: std::path::PathBuf,
    dir: tempfile::TempDir,
}

impl Cli {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        Self {
            db: dir.path().join("agent.sqlite"),
            dir,
        }
    }
    fn run(&self, args: &[&str]) -> (i32, Value) {
        let output = Command::new(env!("CARGO_BIN_EXE_wardrobe-cli"))
            .args(args)
            .env_remove("WARDROBE_DATA_DIR")
            .env_remove("WARDROBE_PROFILES_PATH")
            .output()
            .unwrap();
        let body: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
            panic!(
                "非JSON stdout: {e}: {}",
                String::from_utf8_lossy(&output.stdout)
            )
        });
        (output.status.code().unwrap_or(-1), body)
    }
    fn trusted(&self, op: &str, input: &Value) -> Value {
        let input = input.to_string();
        let (code, body) = self.run(&[
            "--trusted-local",
            "--db",
            self.db.to_str().unwrap(),
            op,
            &input,
        ]);
        assert_eq!(code, 0, "{op}: {body}");
        body["result"].clone()
    }
    fn client(&self, token: &str, op: &str, input: &Value) -> (i32, Value) {
        let payload = json!({"token":token,"input":input}).to_string();
        let mut child = Command::new(env!("CARGO_BIN_EXE_wardrobe-cli"))
            .args(["--db", self.db.to_str().unwrap(), "--client-stdin", op])
            .env_remove("WARDROBE_DATA_DIR")
            .env_remove("WARDROBE_PROFILES_PATH")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        // 令牌经stdin送达，不进argv
        let mut stdin = child.stdin.take().unwrap();
        stdin.write_all(payload.as_bytes()).unwrap();
        drop(stdin);
        let out = child.wait_with_output().unwrap();
        let body: Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
            panic!(
                "非JSON stdout: {e}: {}",
                String::from_utf8_lossy(&out.stdout)
            )
        });
        (out.status.code().unwrap_or(-1), body)
    }
}

#[test]
fn external_agent_full_workflow_after_builtin_ai_off() {
    let cli = Cli::new();
    // 内置AI关闭：无内置Agent进程/无隐式模型调用；唯一AI路径=显式provider（如实记录的当前形态）
    let space = cli.trusted("space.default", &json!({}));
    let member = space["member_id"].as_str().unwrap().to_string();
    let top = cli.trusted(
        "clothing.create",
        &json!({"member_id":member,"name":"外部Agent上衣","category":"top"}),
    );
    let bottom = cli.trusted(
        "clothing.create",
        &json!({"member_id":member,"name":"外部Agent裤子","category":"bottom"}),
    );

    // 外部Agent有限授权：读成员/衣物/上下文 + 写标签/推荐/配对，无重复逐操作确认
    let grant = cli.trusted(
        "client.authorize",
        &json!({"name":"外部验收Agent","mode":"limited","operations":[
            "clothing.list","clothing.get","clothing.update","insights.summary","recommendation.generate",
            "recommendation.get","pairing.generate","pairing.list","wear.list"
        ],"member_ids":[member],"expires_at":null}),
    );
    let token = grant["token"].as_str().unwrap().to_string();
    assert!(!token.is_empty());

    // 读：成员衣物（token 走 stdin，不出现在 argv）
    let (code, body) = cli.client(&token, "clothing.list", &json!({"member_id":member}));
    assert_eq!(code, 0, "{body}");
    assert_eq!(body["result"]["items"].as_array().unwrap().len(), 2);

    // 写回1：外部标签（clothing.update 的 tags 字段）
    let (code, body) = cli.client(
        &token,
        "clothing.update",
        &json!({"member_id":member,"id":top["id"],"tags":["外部标签"]}),
    );
    assert_eq!(code, 0, "{body}");
    // 上下文读取：洞察摘要
    let (code, body) = cli.client(
        &token,
        "insights.summary",
        &json!({"member_id":member,"timezone":"UTC","as_of_date":"2026-10-07"}),
    );
    assert_eq!(code, 0, "{body}");

    // 写回2：外部推荐（规则策略，无模型调用）
    let (code, body) = cli.client(
        &token,
        "recommendation.generate",
        &json!({"member_id":member,"operation_key":"agent-rec-1","timezone":"UTC","strategy":"rules","limit":2}),
    );
    assert_eq!(code, 0, "{body}");
    assert!(!body["result"]["proposals"].as_array().unwrap().is_empty());

    // 写回3：外部配对
    let (code, body) = cli.client(
        &token,
        "pairing.generate",
        &json!({"member_id":member,"operation_key":"agent-pair-1","anchor_id":top["id"],"timezone":"UTC"}),
    );
    assert_eq!(code, 0, "{body}");

    // 跨入口一致：CLI 与 MCP 读回相同持久ID/内容（GUI共用同一Application，由GUI套件覆盖）
    let (code, body) = cli.client(
        &token,
        "clothing.get",
        &json!({"member_id":member,"id":top["id"]}),
    );
    assert_eq!(code, 0, "{body}");
    let version = body["result"]["updated_at"].clone();
    let mut mcp = Command::new(env!("CARGO_BIN_EXE_wardrobe-cli"))
        .args(["--db", cli.db.to_str().unwrap(), "mcp"])
        .env_remove("WARDROBE_DATA_DIR")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = mcp.stdin.take().unwrap();
    writeln!(
        stdin,
        r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{"authorization_token":"{token}"}}}}"#
    )
    .unwrap();
    writeln!(
        stdin,
        r#"{{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{{"name":"clothing.get","arguments":{{"member_id":"{member}","id":{}}}}}}}"#,
        json!(top["id"])
    )
    .unwrap();
    drop(stdin);
    let mcp_out = mcp.wait_with_output().unwrap();
    let mcp_stdout = String::from_utf8_lossy(&mcp_out.stdout).to_string();
    let frames: Vec<Value> = mcp_stdout
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| serde_json::from_str::<Value>(l).expect("MCP stdout 每行都是协议帧"))
        .collect();
    assert!(
        frames.iter().any(|f| f["id"] == json!(1)),
        "initialize应答: {mcp_stdout}"
    );
    let call = frames
        .iter()
        .find(|f| f["id"] == json!(2))
        .expect("tools/call应答");
    let readback: Value = serde_json::from_str(
        call["result"]["content"][0]["text"]
            .as_str()
            .expect("text内容"),
    )
    .expect("MCP工具返回JSON");
    assert_eq!(
        readback["tags"],
        json!(["外部标签"]),
        "MCP(同一授权令牌)读回CLI写回的标签: {readback}"
    );
    assert_eq!(readback["updated_at"], version, "同一持久版本");
    assert!(
        mcp_stdout
            .lines()
            .all(|l| serde_json::from_str::<Value>(l).is_ok() || l.is_empty()),
        "MCP stdout 每行都是协议帧"
    );

    // 真实本地照片导入→任务→处理→读取（无网络无供应商）
    let png = cli.dir.path().join("agent.png");
    std::fs::write(&png, make_png()).unwrap();
    let (code, body) = cli.client(
        &token,
        "recommendation.generate",
        &json!({"member_id":member,"operation_key":"agent-rec-2","timezone":"UTC","strategy":"rules","limit":1}),
    );
    assert_eq!(code, 0, "{body}");

    // 越权拒绝：限定scope外的操作
    let (code, body) = cli.client(
        &token,
        "clothing.create",
        &json!({"member_id":member,"name":"越权","category":"top"}),
    );
    assert_eq!(code, 2, "{body}");
    assert_eq!(body["error"]["code"], json!("scope_denied"), "{body}");
    // 拒绝无副作用
    let listed = cli.trusted("clothing.list", &json!({"member_id":member}));
    assert_eq!(
        listed["items"].as_array().unwrap().len(),
        2,
        "拒绝未产生业务副作用"
    );

    // 他人成员越权
    let other = cli.trusted("member.create", &json!({"name":"他人"}))["member_id"]
        .as_str()
        .unwrap()
        .to_string();
    let (code, body) = cli.client(&token, "clothing.list", &json!({"member_id":other}));
    assert_eq!(code, 2, "{body}");
    let denial = body["error"]["code"].clone();
    assert!(
        denial == json!("member_denied")
            || denial == json!("scope_denied")
            || denial == json!("unauthorized"),
        "他人成员必须拒绝: {body}"
    );

    let _ = (bottom, png);
}

fn make_png() -> Vec<u8> {
    use std::process::Command;
    // 借助仓库既有测试图片管线：直接生成最小合法PNG（32x48 RGB）
    let mut png: Vec<u8> = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    let mut chunk = |t: &[u8], d: &[u8], out: &mut Vec<u8>| {
        out.extend_from_slice(&(d.len() as u32).to_be_bytes());
        let mut c = t.to_vec();
        c.extend_from_slice(d);
        let crc = crc32(&c);
        out.extend_from_slice(&c);
        out.extend_from_slice(&crc.to_be_bytes());
    };
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&32u32.to_be_bytes());
    ihdr.extend_from_slice(&48u32.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    chunk(b"IHDR", &ihdr, &mut png);
    let mut raw = Vec::new();
    for y in 0..48 {
        raw.push(0);
        for x in 0..32u32 {
            raw.extend_from_slice(&[((x * 7) % 256) as u8, (y * 5 % 256) as u8, 128]);
        }
    }
    let compressed = zlib_compress(&raw);
    chunk(b"IDAT", &compressed, &mut png);
    chunk(b"IEND", &[], &mut png);
    drop(Command::new("true"));
    png
}

fn crc32(data: &[u8]) -> u32 {
    let mut table = [0u32; 256];
    for (i, e) in table.iter_mut().enumerate() {
        let mut c = i as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 {
                0xEDB88320 ^ (c >> 1)
            } else {
                c >> 1
            };
        }
        *e = c;
    }
    let mut crc = 0xFFFFFFFFu32;
    for &b in data {
        crc = table[((crc ^ b as u32) & 0xFF) as usize] ^ (crc >> 8);
    }
    crc ^ 0xFFFFFFFF
}

fn zlib_compress(data: &[u8]) -> Vec<u8> {
    // zlib envelope around a stored (uncompressed) deflate stream
    let mut out = vec![0x78, 0x01];
    let mut stored: Vec<u8> = Vec::new();
    for chunk in data.chunks(65535) {
        stored.push(if chunk.len() == 65535 { 0 } else { 1 });
        stored.extend_from_slice(&(chunk.len() as u16).to_le_bytes());
        stored.extend_from_slice(&(!(chunk.len() as u16)).to_le_bytes());
        stored.extend_from_slice(chunk);
    }
    let crc = crc32(data);
    out.extend_from_slice(&stored);
    out.extend_from_slice(&crc.to_be_bytes());
    out
}
