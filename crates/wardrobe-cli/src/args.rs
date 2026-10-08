//! CLI 类型化参数解析：在任何存储打开之前把 argv 解析为类型化调用，
//! 语法错误一律 invalid_arguments（exit 2）。

use std::path::PathBuf;
use wardrobe_core::ApiError;

pub enum Input {
    Inline(String),
    InputStdin,
    ClientStdin,
}

pub struct Execution {
    pub db: Option<PathBuf>,
    pub profile: Option<String>,
    pub trusted_local: bool,
    pub command: String,
    pub input: Option<Input>,
}

pub enum ProfileAction {
    Add,
    Remove,
    List,
    Use,
}

pub struct ProfileCommand {
    pub action: ProfileAction,
    pub name: Option<String>,
    pub data_dir: Option<PathBuf>,
    pub force: bool,
}

pub struct LogsCommand {
    pub db: Option<PathBuf>,
    pub profile: Option<String>,
    pub level: Option<String>,
    pub since: Option<String>,
    pub until: Option<String>,
    pub trace_id: Option<String>,
    pub run_id: Option<String>,
    pub operation: Option<String>,
    pub keyword: Option<String>,
    pub limit: Option<u64>,
    pub cursor: Option<String>,
}

pub enum Parsed {
    Help(Option<String>),
    Schema(String),
    Version,
    Mcp { db: Option<PathBuf>, profile: Option<String> },
    Execute(Execution),
    Profile(ProfileCommand),
    Logs(LogsCommand),
}

fn invalid_arguments(message: &str) -> ApiError {
    ApiError::custom("invalid_arguments", message)
}

/// `--db` 无条件消耗其后恰好一个参数作为值；`--` 之后全部按字面量处理；
/// `mcp` 仅在作为命令位置参数时切换传输。
pub fn parse(args: &[String]) -> Result<Parsed, ApiError> {
    let mut db: Option<PathBuf> = None;
    let mut profile: Option<String> = None;
    let mut trusted_local = false;
    let mut input: Option<Input> = None;
    let mut positionals: Vec<&str> = Vec::new();
    let mut help_flag = false;
    let mut version_flag = false;
    let mut literal = false;
    let mut i = 0;
    while i < args.len() {
        let arg = args[i].as_str();
        if !literal {
            match arg {
                "--" => {
                    literal = true;
                    i += 1;
                    continue;
                }
                "--db" => {
                    if db.is_some() {
                        return Err(invalid_arguments("--db 只能出现一次"));
                    }
                    if profile.is_some() {
                        return Err(invalid_arguments("--db 与 --profile 只能选择一种连接方式"));
                    }
                    match args.get(i + 1) {
                        Some(value) => {
                            db = Some(PathBuf::from(value));
                            i += 2;
                            continue;
                        }
                        None => return Err(invalid_arguments("--db 缺少路径参数")),
                    }
                }
                "--profile" => {
                    if profile.is_some() {
                        return Err(invalid_arguments("--profile 只能出现一次"));
                    }
                    match args.get(i + 1) {
                        Some(value) => {
                            profile = Some(value.to_string());
                            i += 2;
                            continue;
                        }
                        None => return Err(invalid_arguments("--profile 缺少名称参数")),
                    }
                }
                "--trusted-local" => {
                    if trusted_local {
                        return Err(invalid_arguments("--trusted-local 只能出现一次"));
                    }
                    trusted_local = true;
                    i += 1;
                    continue;
                }
                "--input-stdin" | "--client-stdin" => {
                    if input.is_some() {
                        return Err(invalid_arguments(
                            "--input-stdin 与 --client-stdin 只能选择一种输入方式",
                        ));
                    }
                    input = Some(if arg == "--input-stdin" {
                        Input::InputStdin
                    } else {
                        Input::ClientStdin
                    });
                    i += 1;
                    continue;
                }
                "--help" | "--version" => {
                    if help_flag || version_flag {
                        return Err(invalid_arguments("元数据命令只能出现一个"));
                    }
                    if arg == "--help" {
                        help_flag = true;
                    } else {
                        version_flag = true;
                    }
                    i += 1;
                    continue;
                }
                other if other.starts_with('-') && other != "-" => {
                    if positionals
                        .first()
                        .is_some_and(|c| *c == "logs" || c.starts_with("profile."))
                    {
                        // logs/profile 子命令自带旗标（--level 等 / --data-dir/--force），由其分支解析
                        positionals.push(arg);
                        i += 1;
                        continue;
                    }
                    return Err(invalid_arguments(&format!("未知参数 {other}")))
                }
                _ => {}
            }
        }
        positionals.push(arg);
        i += 1;
    }
    // 先解析完整 argv，再判定元数据命令：--help/--version 必须独立使用，仅可携带 --db 全局参数
    if help_flag || version_flag {
        if !positionals.is_empty() || trusted_local || input.is_some() {
            return Err(invalid_arguments(
                "--help/--version 必须单独使用，仅可携带全局参数",
            ));
        }
        return Ok(if help_flag {
            Parsed::Help(None)
        } else {
            Parsed::Version
        });
    }
    match positionals.first().copied() {
        Some("help") => {
            if input.is_some() {
                return Err(invalid_arguments("help 不接受标准输入方式"));
            }
            match positionals.len() {
                1 => Ok(Parsed::Help(None)),
                2 => Ok(Parsed::Help(Some(positionals[1].to_string()))),
                _ => Err(invalid_arguments("help 最多接受一个操作名称")),
            }
        }
        Some("schema") => {
            if input.is_some() {
                return Err(invalid_arguments("schema 不接受标准输入方式"));
            }
            match positionals.len() {
                2 => Ok(Parsed::Schema(positionals[1].to_string())),
                1 => Err(invalid_arguments(
                    "schema 需要一个操作名称，例如 schema clothing.create",
                )),
                _ => Err(invalid_arguments("schema 最多接受一个操作名称")),
            }
        }
        Some("mcp") => {
            if positionals.len() > 1 {
                return Err(invalid_arguments("mcp 不接受输入参数"));
            }
            if input.is_some() {
                return Err(invalid_arguments(
                    "mcp 传输不接受 --input-stdin/--client-stdin",
                ));
            }
            Ok(Parsed::Mcp { db, profile })
        }
        Some("logs") => {
            if input.is_some() {
                return Err(invalid_arguments("logs 子命令不接受标准输入方式"));
            }
            // 过滤参数命名与 diagnostics.logs.query 输入字段对齐（trace-id/run-id 为 kebab 形式）
            const FILTERS: &[&str] = &[
                "--level", "--since", "--until", "--trace-id", "--run-id",
                "--operation", "--keyword", "--limit", "--cursor",
            ];
            let mut filters: Vec<(&str, String)> = Vec::new();
            let mut rest: Vec<&str> = positionals[1..].to_vec();
            while let Some(arg) = rest.first().copied() {
                let Some(flag) = FILTERS.iter().copied().find(|flag| *flag == arg) else {
                    return Err(invalid_arguments(&format!("logs 子命令不支持参数 {arg}")));
                };
                if filters.iter().any(|(seen, _)| *seen == &flag[2..]) {
                    return Err(invalid_arguments(&format!("{flag} 只能出现一次")));
                }
                match rest.get(1) {
                    Some(value) => filters.push((&flag[2..], value.to_string())),
                    None => return Err(invalid_arguments(&format!("{flag} 缺少值"))),
                }
                rest.drain(..2);
            }
            let value = |name: &str| {
                filters
                    .iter()
                    .find(|(seen, _)| *seen == name)
                    .map(|(_, value)| value.clone())
            };
            let limit = match value("limit") {
                None => None,
                Some(text) => match text.parse::<u64>() {
                    Ok(limit) => Some(limit),
                    Err(_) => return Err(invalid_arguments("--limit 必须为整数")),
                },
            };
            Ok(Parsed::Logs(LogsCommand {
                db,
                profile,
                level: value("level"),
                since: value("since"),
                until: value("until"),
                trace_id: value("trace-id"),
                run_id: value("run-id"),
                operation: value("operation"),
                keyword: value("keyword"),
                limit,
                cursor: value("cursor"),
            }))
        }
        Some(command) if command.starts_with("profile.") => {
            let action = match command {
                "profile.add" => ProfileAction::Add,
                "profile.remove" => ProfileAction::Remove,
                "profile.list" => ProfileAction::List,
                "profile.use" => ProfileAction::Use,
                _ => return Err(invalid_arguments("profile子命令仅支持 add/remove/list/use")),
            };
            if input.is_some() {
                return Err(invalid_arguments("profile子命令不接受标准输入方式"));
            }
            let mut name: Option<String> = None;
            let mut data_dir: Option<PathBuf> = None;
            let mut force = false;
            let mut rest: Vec<&str> = positionals[1..].to_vec();
            while let Some(arg) = rest.first().copied() {
                match arg {
                    "--data-dir" => {
                        if data_dir.is_some() {
                            return Err(invalid_arguments("--data-dir 只能出现一次"));
                        }
                        match rest.get(1) {
                            Some(value) => {
                                data_dir = Some(PathBuf::from(value));
                                rest.drain(..2);
                            }
                            None => return Err(invalid_arguments("--data-dir 缺少路径参数")),
                        }
                    }
                    "--force" => {
                        force = true;
                        rest.remove(0);
                    }
                    other if other.starts_with('-') && other != "-" => {
                        return Err(invalid_arguments(&format!("未知参数 {other}")))
                    }
                    _ => {
                        if name.is_some() {
                            return Err(invalid_arguments("多余的位置参数"));
                        }
                        name = Some(arg.to_string());
                        rest.remove(0);
                    }
                }
            }
            if profile.is_some() {
                return Err(invalid_arguments("profile子命令不接受 --profile"));
            }
            match action {
                ProfileAction::List => {
                    if name.is_some() || data_dir.is_some() || force {
                        return Err(invalid_arguments("profile.list 不接受参数"));
                    }
                }
                ProfileAction::Add => {
                    if name.is_none() || data_dir.is_none() {
                        return Err(invalid_arguments(
                            "profile.add 需要 <名称> 与 --data-dir <路径>",
                        ));
                    }
                }
                _ => {
                    if name.is_none() {
                        return Err(invalid_arguments("该profile子命令需要一个名称"));
                    }
                    if data_dir.is_some() {
                        return Err(invalid_arguments("该profile子命令不接受 --data-dir"));
                    }
                }
            }
            Ok(Parsed::Profile(ProfileCommand {
                action,
                name,
                data_dir,
                force,
            }))
        }
        Some(command) => {
            if positionals.len() > 2 {
                return Err(invalid_arguments("多余的位置参数"));
            }
            match positionals.get(1).copied() {
                Some(inline) => {
                    if input.is_some() {
                        return Err(invalid_arguments("JSON 参数与标准输入方式不能同时使用"));
                    }
                    Ok(Parsed::Execute(Execution {
                        db,
                        profile,
                        trusted_local,
                        command: command.to_string(),
                        input: Some(Input::Inline(inline.to_string())),
                    }))
                }
                None => Ok(Parsed::Execute(Execution {
                    db,
                    profile,
                    trusted_local,
                    command: command.to_string(),
                    input,
                })),
            }
        }
        None => {
            if input.is_some() {
                return Err(invalid_arguments("标准输入方式需要一个操作"));
            }
            Ok(Parsed::Execute(Execution {
                db,
                profile,
                trusted_local,
                command: "capabilities".into(),
                input: None,
            }))
        }
    }
}
