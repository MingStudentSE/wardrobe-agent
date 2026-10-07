use serde_json::{json, Value};
use std::{
    io::{self, BufRead, Read},
    path::PathBuf,
};
use wardrobe_core::{ApiError, Application};
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let db = args
        .windows(2)
        .find(|a| a[0] == "--db")
        .map(|a| PathBuf::from(&a[1]))
        .unwrap_or_else(wardrobe_core::default_database_path);
    let app = match Application::open(&db) {
        Ok(app) => app,
        Err(e) => {
            println!("{}", json!({"error":e}));
            std::process::exit(1);
        }
    };
    if args.iter().any(|a| a == "scheduler") {
        if !args.iter().any(|a| a == "--trusted-local") {
            fail(ApiError {
                code: "scope_denied".into(),
                message: "本机调度服务须显式--trusted-local；外部客户端使用成员限定schedule API"
                    .into(),
            });
        }
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
            if args.iter().any(|a| a == "--once") {
                return;
            }
            std::thread::sleep(std::time::Duration::from_secs(5));
        }
    }
    if args.iter().any(|a| a == "mcp") {
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
        return;
    }
    let positional: Vec<&str> = args
        .iter()
        .enumerate()
        .filter(|(i, a)| {
            a.as_str() != "--trusted-local"
                && a.as_str() != "--db"
                && (*i == 0 || args[*i - 1] != "--db")
        })
        .map(|(_, a)| a.as_str())
        .collect();
    let command = positional.first().copied().unwrap_or("capabilities");
    if (command == "provider.credential.set" || command.starts_with("encryption."))
        && !matches!(
            positional.get(1),
            Some(&"--input-stdin" | &"--client-stdin")
        )
    {
        fail(ApiError {
            code: "secret_requires_stdin".into(),
            message: "凭据命令使用--input-stdin，从标准输入读取密钥，避免放入命令历史和进程参数"
                .into(),
        });
    }
    let stdin_input = if matches!(
        positional.get(1),
        Some(&"--input-stdin" | &"--client-stdin")
    ) {
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
    } else {
        None
    };
    let input = match stdin_input
        .as_deref()
        .or_else(|| positional.get(1).copied())
    {
        Some(s) => serde_json::from_str(s),
        None => Ok(json!({})),
    };
    match input {
        Ok(input) => match if args.iter().any(|a| a == "--trusted-local") {
            app.execute(command, input)
        } else if positional.get(1) == Some(&"--client-stdin") {
            app.execute_as_client(
                input["token"].as_str().unwrap_or(""),
                command,
                input.get("input").cloned().unwrap_or(json!({})),
            )
        } else if command == "capabilities" {
            app.execute(command, input)
        } else {
            app.execute_as_client("", command, input)
        } {
            Ok(result) => println!("{}", json!({"result":result})),
            Err(e) => fail(e),
        },
        Err(_) => fail(ApiError {
            code: "invalid_json".into(),
            message: "输入不是有效 JSON".into(),
        }),
    }
}
fn fail(error: ApiError) {
    println!("{}", json!({"error":error}));
    std::process::exit(2);
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
            json!({"protocolVersion":"2024-11-05","capabilities":{"tools":{},"resources":{}},"serverInfo":{"name":"wardrobe-agent","version":"0.1.0"}})
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
