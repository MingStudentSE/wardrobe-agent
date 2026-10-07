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
    pub trusted_local: bool,
    pub command: String,
    pub input: Option<Input>,
}

pub enum Parsed {
    Help(Option<String>),
    Schema(String),
    Version,
    Mcp { db: Option<PathBuf> },
    Scheduler { db: Option<PathBuf>, once: bool },
    Execute(Execution),
}

fn invalid_arguments(message: &str) -> ApiError {
    ApiError {
        code: "invalid_arguments".into(),
        message: message.into(),
    }
}

/// `--db` 无条件消耗其后恰好一个参数作为值；`--` 之后全部按字面量处理；
/// `mcp` 仅在作为命令位置参数时切换传输。
pub fn parse(args: &[String]) -> Result<Parsed, ApiError> {
    let mut db: Option<PathBuf> = None;
    let mut trusted_local = false;
    let mut input: Option<Input> = None;
    let mut positionals: Vec<&str> = Vec::new();
    let mut help_flag = false;
    let mut version_flag = false;
    let mut once = false;
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
                    match args.get(i + 1) {
                        Some(value) => {
                            db = Some(PathBuf::from(value));
                            i += 2;
                            continue;
                        }
                        None => return Err(invalid_arguments("--db 缺少路径参数")),
                    }
                }
                "--once" => {
                    if once {
                        return Err(invalid_arguments("--once 只能出现一次"));
                    }
                    once = true;
                    i += 1;
                    continue;
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
                    return Err(invalid_arguments(&format!("未知参数 {other}")))
                }
                _ => {}
            }
        }
        positionals.push(arg);
        i += 1;
    }
    if once && positionals.first().copied() != Some("scheduler") {
        return Err(invalid_arguments("--once 仅用于 scheduler"));
    }
    // 先解析完整 argv，再判定元数据命令：--help/--version 必须独立使用，仅可携带 --db 全局参数
    if help_flag || version_flag {
        if !positionals.is_empty() || trusted_local || input.is_some() {
            return Err(invalid_arguments(
                "--help/--version 必须单独使用，仅可与 --db 全局参数一起出现",
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
        Some("scheduler") => {
            if !trusted_local {
                return Err(ApiError {
                    code: "scope_denied".into(),
                    message:
                        "本机调度服务须显式--trusted-local；外部客户端使用成员限定schedule API"
                            .into(),
                });
            }
            if positionals.len() != 1 || input.is_some() {
                return Err(invalid_arguments("scheduler 不接受输入参数"));
            }
            Ok(Parsed::Scheduler { db, once })
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
            Ok(Parsed::Mcp { db })
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
                        trusted_local,
                        command: command.to_string(),
                        input: Some(Input::Inline(inline.to_string())),
                    }))
                }
                None => Ok(Parsed::Execute(Execution {
                    db,
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
                trusted_local,
                command: "capabilities".into(),
                input: None,
            }))
        }
    }
}
