//! 出站 HTTP 客户端唯一工厂：连接/总超时与重定向策略按用途档案在此集中登记。
//! 新增出站调用一律选择既有档案或在此新增一行，不再各自拼 builder；
//! 构建失败映射为哪个业务错误仍归调用方（各链路错误码语义不同）。

/// 出站客户端档案：一行一个真实用途，超时与重定向策略只有此处一处事实来源。
pub(crate) enum OutboundProfile {
    /// 模型 chat/completions 文本通道（推荐意图、着装问答、场景规划、录入识别、购买回执）。
    Chat,
    /// 供应商连接验证（轻量探测请求）。
    Probe,
    /// 商品图生成等长任务（等待供应商出图）。
    Image,
    /// 天气查询（快速失败，可回退缓存或手动温度）。
    Weather,
    /// 本地抠图模型权重下载（允许跟随下载源重定向）。
    ModelDownload,
    /// S3 兼容对象存储（重定向意味着端点或区域配置不符，按显式错误返回）。
    ObjectStorage,
    /// WebDAV 存储：总超时来自同步配置，连接超时与之相同；允许跟随重定向。
    WebDav { timeout: std::time::Duration },
}

pub(crate) fn build(profile: OutboundProfile) -> Result<reqwest::blocking::Client, reqwest::Error> {
    let builder = reqwest::blocking::Client::builder();
    match profile {
        OutboundProfile::Chat => builder
            .timeout(std::time::Duration::from_secs(60))
            .redirect(reqwest::redirect::Policy::none())
            .build(),
        OutboundProfile::Probe => builder
            .timeout(std::time::Duration::from_secs(20))
            .redirect(reqwest::redirect::Policy::none())
            .build(),
        OutboundProfile::Image => builder
            .timeout(std::time::Duration::from_secs(240))
            .redirect(reqwest::redirect::Policy::none())
            .build(),
        OutboundProfile::Weather => builder
            .timeout(std::time::Duration::from_secs(15))
            .redirect(reqwest::redirect::Policy::none())
            .build(),
        OutboundProfile::ModelDownload => builder
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(90))
            .build(),
        OutboundProfile::ObjectStorage => builder
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(60))
            .redirect(reqwest::redirect::Policy::none())
            .build(),
        OutboundProfile::WebDav { timeout } => {
            builder.connect_timeout(timeout).timeout(timeout).build()
        }
    }
}
