//! wardrobe-web：本机浏览器交付服务（ADR0026）。
//! 只绑定 127.0.0.1；数据与桌面/CLI 完全一致；无公网监听、无账号体系。

use std::path::PathBuf;

fn usage() -> String {
    format!(
        "wardrobe-web {}\n\n本机浏览器交付服务：把共享衣橱 Application API 以同源 JSON API 与静态界面提供给本机浏览器。\n\n用法：\n  wardrobe-web [--port <端口>] [--data-dir <数据目录>] [--web-dist <界面目录>]\n\n选项：\n  --port <端口>       监听端口（默认 17838；始终只绑定 127.0.0.1）\n  --data-dir <目录>   数据目录（默认与桌面/CLI 一致；WARDROBE_DATA_DIR 优先于默认值）\n  --web-dist <目录>   React 构建产物目录（默认环境变量 WARDROBE_WEB_DIST，再退回可执行文件旁 web-dist）\n  --help              显示本帮助\n  --version           显示版本\n",
        env!("CARGO_PKG_VERSION")
    )
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("{}", usage());
        return;
    }
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!(
            "{}",
            serde_json::json!({"schema_version":1,"result":{"name":"wardrobe-web","version":env!("CARGO_PKG_VERSION")}})
        );
        return;
    }
    let mut port: u16 = 17838;
    let mut data_dir: Option<PathBuf> = None;
    let mut web_dist: Option<PathBuf> = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--port" => {
                index += 1;
                port = args
                    .get(index)
                    .and_then(|value| value.parse().ok())
                    .unwrap_or_else(|| {
                        eprintln!("--port 需要一个端口号");
                        std::process::exit(2);
                    });
            }
            "--data-dir" => {
                index += 1;
                data_dir = args.get(index).map(PathBuf::from);
            }
            "--web-dist" => {
                index += 1;
                web_dist = args.get(index).map(PathBuf::from);
            }
            other => {
                eprintln!("未知参数：{other}\n\n{}", usage());
                std::process::exit(2);
            }
        }
        index += 1;
    }
    let data_dir = data_dir.or_else(|| {
        std::env::var_os("WARDROBE_DATA_DIR").map(|path| PathBuf::from(path))
    });
    let data_dir = match data_dir {
        Some(directory) => directory,
        None => wardrobe_core::default_database_path()
            .parent()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(".")),
    };
    let web_dist = web_dist
        .or_else(|| std::env::var_os("WARDROBE_WEB_DIST").map(PathBuf::from))
        .or_else(|| {
            std::env::current_exe()
                .ok()
                .and_then(|exe| exe.parent().map(|dir| dir.join("web-dist")))
                .filter(|path| path.is_dir())
        });
    let config = wardrobe_web::Config::new(port, data_dir);
    let config = match web_dist {
        Some(directory) => config.with_dist(directory),
        None => config,
    };
    let running = wardrobe_web::spawn(config).unwrap_or_else(|error| {
        eprintln!("[wardrobe-web] 启动失败：{error}");
        std::process::exit(1);
    });
    eprintln!(
        "[wardrobe-web] 请用本机浏览器打开 http://127.0.0.1:{}/ （本服务不监听公网）",
        running.port
    );
    loop {
        std::thread::sleep(std::time::Duration::from_secs(3600));
    }
}
