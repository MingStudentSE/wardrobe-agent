use serde_json::{json, Value};
use std::{
    io::{self, BufRead},
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
    if args.iter().any(|a| a == "mcp") {
        for line in io::stdin().lock().lines() {
            match line {
                Ok(line) => {
                    if let Some(response) = mcp(&app, &line) {
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
        .filter(|(i, a)| a.as_str() != "--db" && (*i == 0 || args[*i - 1] != "--db"))
        .map(|(_, a)| a.as_str())
        .collect();
    let command = positional.first().copied().unwrap_or("capabilities");
    let input = match positional.get(1) {
        Some(s) => serde_json::from_str(s),
        None => Ok(json!({})),
    };
    match input {
        Ok(input) => match app.execute(command, input) {
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
fn mcp(app: &Application, line: &str) -> Option<Value> {
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
            json!({"protocolVersion":"2024-11-05","capabilities":{"tools":{},"resources":{}},"serverInfo":{"name":"wardrobe-agent","version":"0.1.0"}})
        }
        "ping" => json!({}),
        "tools/list" => json!({"tools":wardrobe_core::operations()}),
        "resources/list" => {
            json!({"resources":[{"uri":"wardrobe://capabilities","name":"Application capabilities","mimeType":"application/json"}]})
        }
        "resources/read" => {
            if request["params"]["uri"] != "wardrobe://capabilities" {
                return Some(
                    json!({"jsonrpc":"2.0","id":id,"error":{"code":-32602,"message":"Unknown resource"}}),
                );
            }
            json!({"contents":[{"uri":"wardrobe://capabilities","mimeType":"application/json","text":app.execute("capabilities",json!({})).unwrap().to_string()}]})
        }
        "tools/call" => match app.execute(
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
