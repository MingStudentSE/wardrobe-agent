//! 本机 Web 服务：把共享 wardrobe-core Application API 以同源 JSON API
//! 与静态 React 界面提供给本机浏览器（ADR0026）。
//! 只绑定 127.0.0.1；一切业务经 `execute_as_client`；本模块不新增第二套业务逻辑。

pub mod service;
pub use service::{spawn, Config, RunningService};
