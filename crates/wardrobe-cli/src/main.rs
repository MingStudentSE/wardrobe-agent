mod args;
use serde_json::{json, Value};
use std::{
    io::{self, BufRead, Read},
    path::PathBuf,
};
use wardrobe_core::{ApiError, Application};
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let parsed = match args::parse(&args) {
        Err(error) => fail(error),
        Ok(parsed) => parsed,
    };
    match parsed {
        args::Parsed::Version => {
            println!(
                "{}",
                json!({"schema_version":1,"result":{"name":"wardrobe-cli","version":env!("CARGO_PKG_VERSION")}})
            );
        }
        args::Parsed::Help(None) => {
            println!("{}", json!({"schema_version":1,"result":help()}));
        }
        args::Parsed::Help(Some(operation)) => print_operation(operation),
        args::Parsed::Schema(operation) => print_operation(operation),
        args::Parsed::Mcp { db } => run_mcp(db),
        args::Parsed::Scheduler { db, once } => run_scheduler(db, once),
        args::Parsed::Execute(execution) => execute(execution),
    }
}
fn run_scheduler(db: Option<PathBuf>, once: bool) {
    let db = db.unwrap_or_else(wardrobe_core::default_database_path);
    use std::io::Write;
    println!(
        "{}",
        json!({"service":"scheduler","state":"running","notifications":"native_host_required","pid":std::process::id()})
    );
    let _ = io::stdout().flush();
    loop {
        match Application::open(&db).and_then(|app| {
            if !app.schedule_host_should_run(false)? {
                return Ok(json!({"queued":[],"results":[],"paused":true}));
            }
            let due = app.schedule_tick_at(chrono::Utc::now())?;
            let pending = app.schedule_pending_jobs()?;
            let mut results = vec![];
            for job in pending {
                let value = app.execute(
                    "schedule.worker.run",
                    json!({"member_id":job["member_id"],"id":job["id"]}),
                );
                results.push(match value {
                    Ok(job) => json!({"id":job["id"],"state":job["state"]}),
                    Err(error) => json!({"id":job["id"],"error":error}),
                });
            }
            Ok(json!({"queued":due["queued"],"results":results}))
        }) {
            Ok(value) => {
                if !value["queued"].as_array().unwrap().is_empty()
                    || !value["results"].as_array().unwrap().is_empty()
                {
                    println!("{value}");
                }
            }
            Err(error) => println!("{}", json!({"error":error})),
        }
        let _ = io::stdout().flush();
        if once {
            return;
        }
        std::thread::sleep(std::time::Duration::from_secs(5));
    }
}
fn run_mcp(db: Option<PathBuf>) {
    let db = db.unwrap_or_else(wardrobe_core::default_database_path);
    let app = match Application::open(&db) {
        Ok(app) => app,
        Err(e) => {
            // MCP 流 stdout 只允许 JSON-RPC 行；启动失败走 stderr 非secret诊断
            eprintln!(
                "wardrobe-cli mcp 启动失败: code={} message={}",
                e.code, e.message
            );
            std::process::exit(1);
        }
    };

    let mut client_token = None;
    for line in io::stdin().lock().lines() {
        match line {
            Ok(line) => {
                if let Some(response) = mcp(&app, &line, &mut client_token) {
                    println!("{response}");
                }
            }
            Err(_) => break,
        }
    }
}
fn execute(execution: args::Execution) {
    if (execution.command == "provider.credential.set"
        || execution.command.starts_with("encryption."))
        && !matches!(
            execution.input,
            Some(args::Input::InputStdin | args::Input::ClientStdin)
        )
    {
        fail(ApiError {
            code: "secret_requires_stdin".into(),
            message: "凭据命令使用--input-stdin，从标准输入读取密钥，避免放入命令历史和进程参数"
                .into(),
        });
    }
    let db = execution
        .db
        .unwrap_or_else(wardrobe_core::default_database_path);
    let app = match Application::open(&db) {
        Ok(app) => app,
        Err(e) => {
            println!("{}", json!({"schema_version":1,"error":e}));
            std::process::exit(1);
        }
    };
    let client_stdin = matches!(execution.input, Some(args::Input::ClientStdin));
    let stdin_body = match execution.input {
        Some(args::Input::Inline(text)) => Some(text),
        Some(args::Input::InputStdin | args::Input::ClientStdin) => {
            let mut body = String::new();
            if io::stdin()
                .take(1_048_577)
                .read_to_string(&mut body)
                .is_err()
                || body.len() > 1_048_576
            {
                fail(ApiError {
                    code: "invalid_input".into(),
                    message: "无法读取标准输入或输入过大".into(),
                });
            }
            Some(body)
        }
        None => None,
    };
    let input = match stdin_body.as_deref() {
        Some(text) => match serde_json::from_str(text) {
            Ok(value) => value,
            Err(_) => fail(ApiError {
                code: "invalid_json".into(),
                message: "输入不是有效 JSON".into(),
            }),
        },
        None => json!({}),
    };
    let result = if execution.trusted_local {
        app.execute(&execution.command, input)
    } else if client_stdin {
        app.execute_as_client(
            input["token"].as_str().unwrap_or(""),
            &execution.command,
            input.get("input").cloned().unwrap_or(json!({})),
        )
    } else if execution.command == "capabilities" {
        app.execute(&execution.command, input)
    } else {
        app.execute_as_client("", &execution.command, input)
    };
    match result {
        Ok(result) => println!("{}", json!({"schema_version":1,"result":result})),
        // 存储失败 exit 1；业务/校验拒绝走 fail() exit 2。MCP 传输语义不变。
        Err(e) if e.code == "storage_error" => {
            println!("{}", json!({"schema_version":1,"error":e}));
            std::process::exit(1)
        }
        Err(e) => fail(e),
    }
}
fn fail(error: ApiError) -> ! {
    println!("{}", json!({"schema_version":1,"error":error}));
    std::process::exit(2)
}
fn print_operation(operation: String) {
    match operation_metadata(&operation) {
        Ok(descriptor) => println!("{}", json!({"schema_version":1,"result":descriptor})),
        Err(error) => fail(error),
    }
}
fn operation_metadata(operation: &str) -> Result<Value, ApiError> {
    wardrobe_core::operations()
        .as_array()
        .and_then(|descriptors| {
            descriptors
                .iter()
                .find(|descriptor| descriptor["name"].as_str() == Some(operation))
                .cloned()
        })
        .ok_or(ApiError {
            code: "unsupported_operation".into(),
            message: format!("未知操作 {operation}"),
        })
}
fn help() -> Value {
    json!({
        "version": env!("CARGO_PKG_VERSION"),
        "usage": [
            "wardrobe-cli --help",
            "wardrobe-cli --version",
            "wardrobe-cli help <operation>",
            "wardrobe-cli schema <operation>",
            "wardrobe-cli [--db <数据库路径>] [--trusted-local] <operation> ['<JSON>' | --input-stdin | --client-stdin]",
            "wardrobe-cli [--db <数据库路径>] mcp"
        ],
        "flags": {
            "--db": "指定数据库文件路径；缺省使用 WARDROBE_DATA_DIR/wardrobe.sqlite，其次系统应用数据目录",
            "--trusted-local": "本机人工维护通道：绕过客户端授权，以本机信任身份直接执行维护操作；外部 AI 一律使用 --client-stdin 的 scope 限定客户端路径",
            "--input-stdin": "从标准输入读取操作 JSON",
            "--client-stdin": "从标准输入读取 {\"token\":\"连接令牌\",\"input\":{…}}，以外部客户端授权执行",
            "--": "其后所有参数按字面量处理"
        },
        "authorization": "外部 AI 客户端通过 --client-stdin 提供连接令牌，令牌不得放入命令行参数；授权在到期、撤销与 scope 范围内可重复使用；凭据与加密命令仅接受标准输入",
        "data": "数据目录可用 --db 或环境变量 WARDROBE_DATA_DIR 指定；未指定时使用系统应用数据目录，数据库随应用迁移机制移动"
    })
}
fn mcp(app: &Application, line: &str, client_token: &mut Option<String>) -> Option<Value> {
    let request: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(_) => {
            return Some(
                json!({"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":"Parse error"}}),
            )
        }
    };
    let id = request.get("id")?.clone();
    let result = match request["method"].as_str().unwrap_or("") {
        "initialize" => {
            if let Some(token) = request["params"]["authorization_token"].as_str() {
                if let Err(error) = app.execute_as_client(token, "capabilities", json!({})) {
                    return Some(
                        json!({"jsonrpc":"2.0","id":id,"error":{"code":-32001,"message":error.message,"data":{"code":error.code}}}),
                    );
                }
                *client_token = Some(token.to_string());
            }
            json!({"protocolVersion":"2024-11-05","capabilities":{"tools":{},"resources":{}},"serverInfo":{"name":"wardrobe-agent","version":env!("CARGO_PKG_VERSION")}})
        }
        "ping" => json!({}),
        "tools/list" => match app.execute_as_client(
            client_token.as_deref().unwrap_or(""),
            "capabilities",
            json!({}),
        ) {
            Ok(v) => json!({"tools":v["operations"]}),
            Err(e) => {
                return Some(
                    json!({"jsonrpc":"2.0","id":id,"error":{"code":-32001,"message":e.message,"data":{"code":e.code}}}),
                )
            }
        },
        "resources/list" => {
            json!({"resources":[{"uri":"wardrobe://capabilities","name":"Application capabilities","mimeType":"application/json"}]})
        }
        "resources/read" => {
            if request["params"]["uri"] != "wardrobe://capabilities" {
                return Some(
                    json!({"jsonrpc":"2.0","id":id,"error":{"code":-32602,"message":"Unknown resource"}}),
                );
            }
            match app.execute_as_client(
                client_token.as_deref().unwrap_or(""),
                "capabilities",
                json!({}),
            ) {
                Ok(v) => {
                    json!({"contents":[{"uri":"wardrobe://capabilities","mimeType":"application/json","text":v.to_string()}]})
                }
                Err(e) => {
                    return Some(
                        json!({"jsonrpc":"2.0","id":id,"error":{"code":-32001,"message":e.message,"data":{"code":e.code}}}),
                    )
                }
            }
        }
        "tools/call" => match app.execute_as_client(
            request["params"]["authorization_token"]
                .as_str()
                .or(client_token.as_deref())
                .unwrap_or(""),
            request["params"]["name"].as_str().unwrap_or(""),
            request["params"]
                .get("arguments")
                .cloned()
                .unwrap_or(json!({})),
        ) {
            Ok(v) => json!({"content":[{"type":"text","text":v.to_string()}],"isError":false}),
            Err(e) => {
                json!({"content":[{"type":"text","text":json!({"error":e}).to_string()}],"isError":true})
            }
        },
        _ => {
            return Some(
                json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":"Method not found"}}),
            )
        }
    };
    Some(json!({"jsonrpc":"2.0","id":id,"result":result}))
}
