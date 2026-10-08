use crate::{ApiError, Application};
use rusqlite::{OptionalExtension, Transaction};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
pub(super) const MODEL_ID: &str = "u2netp-rembg-v0";
pub(super) const MODEL_SHA: &str =
    "309c8469258dda742793dce0ebea8e6dd393174f89934733ecc8b14c76f4ddd8";
pub(super) const MODEL_BYTES: u64 = 4_574_861;
pub(super) const MODEL_URL: &str =
    "https://github.com/danielgatis/rembg/releases/download/v0.0.0/u2netp.onnx";
pub(super) fn migrate(tx: &Transaction<'_>) -> Result<(), ApiError> {
    tx.execute_batch(
        "CREATE TABLE IF NOT EXISTS local_models(id TEXT PRIMARY KEY,document TEXT NOT NULL);",
    )?;
    Ok(())
}
impl Application {
    pub(super) fn cutout_model_status(&self, input: Value) -> Result<Value, ApiError> {
        self.member(&input)?;
        let path = self
            .asset_root
            .join("models-local-only")
            .join(format!("{MODEL_ID}.onnx"));
        let ready = std::fs::read(&path).ok().is_some_and(|bytes| {
            bytes.len() as u64 == MODEL_BYTES
                && Sha256::digest(&bytes)
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>()
                    == MODEL_SHA
        });
        let stored: Option<String> = self
            .connection
            .query_row(
                "SELECT document FROM local_models WHERE id=?1",
                [MODEL_ID],
                |r| r.get(0),
            )
            .optional()?;
        let mut prior: Value = stored
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or(json!({"state":"missing","progress":0,"error":null}));
        if !ready {
            if path.exists() {
                prior = json!({"state":"failed","progress":0,"error":{"code":"model_checksum_mismatch","message":"本地模型校验不符，请重新下载"}});
            } else if prior["state"] == "ready" {
                prior = json!({"state":"missing","progress":0,"error":null});
            } else if prior["state"] == "downloading" {
                let job_id = prior["job_id"].as_str().unwrap_or("");
                let stored: Option<String> = self
                    .connection
                    .query_row("SELECT document FROM jobs WHERE id=?1", [job_id], |r| {
                        r.get(0)
                    })
                    .optional()?;
                let running = stored
                    .and_then(|s| serde_json::from_str::<Value>(&s).ok())
                    .is_some_and(|job| {
                        job["state"] == "running"
                            && job["lease_expires"].as_i64().unwrap_or(0)
                                > chrono::Utc::now().timestamp_millis()
                    });
                if !running {
                    prior = json!({"state":"missing","progress":0,"error":{"code":"download_interrupted","message":"上次模型下载未完成，可继续或重试任务"}});
                }
            }
        }
        Ok(
            json!({"schema_version":1,"model_id":MODEL_ID,"state":if ready{"ready"}else{prior["state"].as_str().unwrap_or("missing")},"progress":if ready{100}else{prior["progress"].as_u64().unwrap_or(0)},"error":if ready{Value::Null}else{prior["error"].clone()},"bytes":MODEL_BYTES,"sha256":MODEL_SHA,"license":"Apache-2.0","source_url":MODEL_URL,"cached_offline":ready}),
        )
    }
}
impl Application {
    pub(super) fn cutout_enqueue(&self, operation: &str, input: Value) -> Result<Value, ApiError> {
        let member = self.member(&input)?;
        let key = crate::string(&input, "operation_key")?;
        if key.is_empty() || key.len() > 120 {
            return Err(ApiError::new("invalid_input", "操作标识须为1–120字符"));
        }
        let payload = if operation == "images.cutout" {
            let asset =
                self.asset_get(json!({"member_id":member,"id":crate::string(&input,"asset_id")?}))?;
            json!({"asset_id":asset["id"],"source_display_id":asset["display"]["id"],"model_id":MODEL_ID})
        } else {
            json!({"model_id":MODEL_ID})
        };
        self.job_enqueue(
            &member,
            if operation == "images.cutout" {
                "local_cutout"
            } else {
                "local_model_download"
            },
            key,
            payload,
        )
    }
    pub(super) fn cutout_run(&self, input: Value) -> Result<Value, ApiError> {
        let member = self.member(&input)?;
        let id = crate::string(&input, "job_id")?;
        let job = self.job_get(json!({"member_id":member,"id":id}))?;
        if !matches!(
            job["kind"].as_str(),
            Some("local_cutout" | "local_model_download")
        ) {
            return Err(ApiError::new("invalid_input", "非本地图片处理任务"));
        }
        if job["state"] == "succeeded" {
            return Ok(job);
        }
        let job = self.job_claim(&member, id, 300)?;
        let lease = job["lease_token"].as_str().unwrap();
        if job["kind"] == "local_model_download" {
            let result = self.cutout_download(&member, id, lease);
            return match result {
                Ok(v) => self.job_finish(&member, id, lease, "succeeded", v, Value::Null),
                Err(e) => {
                    let status = self.cutout_model_status(json!({"member_id":member}))?;
                    self.job_finish(&member,id,lease,"failed",Value::Null,json!({"code":e.code,"message":e.message,"retryable":matches!(e.code.as_str(),"download_offline"|"download_retryable"),"retry_after_ms":status["error"]["retry_after_ms"]}))
                }
            };
        }
        let result = self.cutout_compute(&member, &job);
        match result {
            Err(e) => self.job_finish(
                &member,
                id,
                lease,
                "failed",
                Value::Null,
                json!({"code":e.code,"message":e.message,"retryable":false}),
            ),
            Ok((source, image)) => {
                let tx = self.begin_write()?;
                let mut current = self.job_get(json!({"member_id":member,"id":id}))?;
                if current["state"] != "running"
                    || current["lease_token"] != lease
                    || current["lease_expires"].as_i64().unwrap_or(0)
                        < chrono::Utc::now().timestamp_millis()
                {
                    return Err(ApiError::new(
                        "lease_lost",
                        "处理已取消或超时，旧展示图未改变",
                    ));
                }
                let asset=self.image_derived(&tx,source,image,"cutout",json!({"model_id":MODEL_ID,"model_sha256":MODEL_SHA,"runtime":"tract-onnx-0.23.8","job_id":id}))?;
                current["state"] = json!("succeeded");
                current["result"] = asset;
                current["progress"] = json!(100);
                current["error"] = Value::Null;
                current["lease_token"] = Value::Null;
                current["lease_expires"] = json!(0);
                current["updated_at"] = json!(chrono::Utc::now().to_rfc3339());
                self.save_job(&tx, &current)?;
                tx.commit()?;
                Ok(current)
            }
        }
    }
    fn cutout_download(&self, member: &str, job_id: &str, lease: &str) -> Result<Value, ApiError> {
        use std::io::Read;
        let status = self.cutout_model_status(json!({"member_id":member}))?;
        if status["cached_offline"] == true {
            return Ok(status);
        }
        self.cutout_model_record(
            json!({"state":"downloading","progress":0,"error":null,"job_id":job_id}),
        )?;
        let mut retry_after_ms = None;
        let result = (|| {
            let client = reqwest::blocking::Client::builder()
                .connect_timeout(std::time::Duration::from_secs(10))
                .timeout(std::time::Duration::from_secs(90))
                .build()
                .map_err(|_| {
                    ApiError::new(
                        "download_offline",
                        "无法连接模型下载服务；已缓存模型仍可离线使用",
                    )
                })?;
            let mut response = client
                .get(MODEL_URL)
                .send()
                .map_err(|_| ApiError::new("download_offline", "模型下载未连接；联网后重试"))?;
            if !response.status().is_success() {
                retry_after_ms = response
                    .headers()
                    .get(reqwest::header::RETRY_AFTER)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| {
                        v.parse::<i64>()
                            .ok()
                            .map(|seconds| seconds.saturating_mul(1000))
                            .or_else(|| {
                                chrono::DateTime::parse_from_rfc2822(v).ok().map(|date| {
                                    date.timestamp_millis()
                                        .saturating_sub(chrono::Utc::now().timestamp_millis())
                                })
                            })
                    })
                    .map(|milliseconds| milliseconds.clamp(1000, 300_000));
                return Err(ApiError::new(
                    if response.status().is_server_error() || response.status().as_u16() == 429 {
                        "download_retryable"
                    } else {
                        "download_failed"
                    },
                    "模型下载服务返回错误；旧图保留",
                ));
            }
            let mut bytes = Vec::with_capacity(MODEL_BYTES as usize);
            let mut chunk = [0u8; 64 * 1024];
            let mut recorded = 0;
            loop {
                let read = response
                    .read(&mut chunk)
                    .map_err(|_| ApiError::new("download_offline", "模型下载中断；联网后重试"))?;
                if read == 0 {
                    break;
                }
                bytes.extend_from_slice(&chunk[..read]);
                if bytes.len() as u64 > MODEL_BYTES {
                    return Err(ApiError::new(
                        "model_checksum_mismatch",
                        "模型大小不符，未启用",
                    ));
                }
                let progress = bytes.len() as u64 * 100 / MODEL_BYTES;
                if progress >= recorded + 5 {
                    let current = self.job_get(json!({"member_id":member,"id":job_id}))?;
                    if current["state"] != "running" || current["lease_token"] != lease {
                        return Err(ApiError::new("lease_lost", "模型下载已取消"));
                    }
                    self.cutout_model_record(json!({"state":"downloading","progress":progress,"error":null,"job_id":job_id}))?;
                    recorded = progress;
                }
            }
            if bytes.len() as u64 != MODEL_BYTES
                || Sha256::digest(&bytes)
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>()
                    != MODEL_SHA
            {
                return Err(ApiError::new(
                    "model_checksum_mismatch",
                    "模型校验失败，未启用",
                ));
            }
            let tx = self.begin_write()?;
            let current = self.job_get(json!({"member_id":member,"id":job_id}))?;
            if current["state"] != "running" || current["lease_token"] != lease {
                return Err(ApiError::new("lease_lost", "模型下载已取消"));
            }
            let root = self.asset_root.join("models-local-only");
            std::fs::create_dir_all(&root)
                .map_err(|_| ApiError::new("storage_error", "模型目录写入失败"))?;
            let temp = root.join(format!("{}.tmp", uuid::Uuid::now_v7()));
            std::fs::write(&temp, &bytes)
                .map_err(|_| ApiError::new("storage_error", "模型写入失败"))?;
            std::fs::rename(temp, root.join(format!("{MODEL_ID}.onnx")))
                .map_err(|_| ApiError::new("storage_error", "模型缓存完成失败"))?;
            tx.commit()?;
            Ok(())
        })();
        match result {
            Ok(()) => {
                self.cutout_model_record(json!({"state":"ready","progress":100,"error":null}))?;
                self.cutout_model_status(json!({"member_id":member}))
            }
            Err(e) => {
                self.cutout_model_record(json!({"state":if e.code=="download_offline"{"offline"}else{"failed"},"progress":0,"error":{"code":e.code,"message":e.message,"retry_after_ms":retry_after_ms}}))?;
                Err(e)
            }
        }
    }
    fn cutout_model_record(&self, doc: Value) -> Result<(), ApiError> {
        let tx = self.begin_write()?;
        tx.execute("INSERT INTO local_models VALUES(?1,?2) ON CONFLICT(id) DO UPDATE SET document=excluded.document",rusqlite::params![MODEL_ID,doc.to_string()])?;
        tx.commit()?;
        Ok(())
    }
    fn cutout_compute(
        &self,
        member: &str,
        job: &Value,
    ) -> Result<(Value, image::DynamicImage), ApiError> {
        use tract_onnx::prelude::*;
        if self.cutout_model_status(json!({"member_id":member}))?["cached_offline"] != true {
            return Err(ApiError::new(
                "model_missing",
                "请先下载并校验本地抠图模型；旧展示图保留",
            ));
        }
        let asset = self.asset_get(json!({"member_id":member,"id":job["payload"]["asset_id"]}))?;
        if asset["display"]["id"] != job["payload"]["source_display_id"] {
            return Err(ApiError::new(
                "source_changed",
                "来源图片已改变，请重新创建任务",
            ));
        }
        let image = crate::image_editing::read_display(&asset)?;
        let failure = |_| {
            ApiError::new(
                "local_inference_failed",
                "本地模型处理失败；原图和旧展示图保留",
            )
        };
        let model = tract_onnx::onnx()
            .model_for_path(
                self.asset_root
                    .join("models-local-only")
                    .join(format!("{MODEL_ID}.onnx")),
            )
            .map_err(failure)?
            .with_input_fact(0, f32::fact([1, 3, 320, 320]).into())
            .map_err(failure)?
            .into_optimized()
            .map_err(failure)?
            .into_runnable()
            .map_err(failure)?;
        let rgb = image
            .resize_exact(320, 320, image::imageops::FilterType::Lanczos3)
            .to_rgb8();
        let maximum = rgb.pixels().flat_map(|p| p.0).max().unwrap_or(0) as f32;
        let divisor = maximum.max(1.0);
        let mean = [0.485, 0.456, 0.406];
        let std = [0.229, 0.224, 0.225];
        let input = tract_ndarray::Array4::from_shape_fn((1, 3, 320, 320), |(_, c, y, x)| {
            (rgb.get_pixel(x as u32, y as u32)[c] as f32 / divisor - mean[c]) / std[c]
        });
        let outputs = model
            .run(tvec!(input.into_tensor().into()))
            .map_err(failure)?;
        let prediction = outputs[0].to_plain_array_view::<f32>().map_err(failure)?;
        if prediction.len() != 320 * 320 || prediction.iter().any(|v| !v.is_finite()) {
            return Err(ApiError::new(
                "local_inference_failed",
                "模型输出无效；旧展示图保留",
            ));
        }
        let low = prediction.iter().copied().fold(f32::INFINITY, f32::min);
        let high = prediction.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        if high - low < 1e-7 {
            return Err(ApiError::new(
                "local_inference_failed",
                "模型没有辨认出前景；旧展示图保留",
            ));
        }
        let mask = image::GrayImage::from_raw(
            320,
            320,
            prediction
                .iter()
                .map(|p| ((p - low) / (high - low) * 255.0) as u8)
                .collect(),
        )
        .ok_or_else(|| ApiError::new("local_inference_failed", "抠图蒙版无效"))?;
        let mask = image::imageops::resize(
            &mask,
            image.width(),
            image.height(),
            image::imageops::FilterType::Lanczos3,
        );
        let mut rgba = image.to_rgba8();
        for (pixel, alpha) in rgba.pixels_mut().zip(mask.pixels()) {
            pixel[3] = (u16::from(pixel[3]) * u16::from(alpha[0]) / 255) as u8;
        }
        Ok((asset, image::DynamicImage::ImageRgba8(rgba)))
    }
}
pub(super) fn descriptors() -> Vec<Value> {
    let model = json!({"type":"object","required":["schema_version","model_id","state","progress","error","bytes","sha256","license","source_url","cached_offline"],"properties":{"schema_version":{"const":1},"model_id":{"const":MODEL_ID},"state":{"enum":["missing","downloading","ready","offline","failed"]},"progress":{"type":"integer","minimum":0,"maximum":100},"error":{"type":["object","null"]},"bytes":{"const":MODEL_BYTES},"sha256":{"const":MODEL_SHA},"license":{"const":"Apache-2.0"},"source_url":{"const":MODEL_URL},"cached_offline":{"type":"boolean"}}});
    let job = json!({"type":"object","required":["schema_version","id","member_id","kind","state","progress","payload","result","error","attempts","next_due","created_at","updated_at"],"properties":{"schema_version":{"const":1},"id":{"type":"string"},"member_id":{"type":"string"},"kind":{"enum":["local_cutout","local_model_download"]},"state":{"enum":["queued","running","waiting_input","succeeded","failed","cancelled"]},"progress":{"type":"integer"},"payload":{"type":"object"},"result":{"type":["object","null"]},"error":{"type":["object","null"]},"attempts":{"type":"integer"},"next_due":{"type":"integer"},"created_at":{"type":"string"},"updated_at":{"type":"string"},"trace_id":{"type":"string","description":"创建该任务的应用调用链路标识；worker直接入队的任务缺省"}}});
    ["images.model.status","images.model.download","images.cutout","images.worker.run"].iter().map(|name|{
 let (effect,examples)=match *name {
            "images.model.status"=>(json!({"kind":"query","network":"none","local_files":true,"async":null}),json!([{"request":{"member_id":"<已授权成员ID>"},"prerequisites":["读取本机模型缓存状态与校验信息；不联网"]}])),"images.model.download"=>(json!({"kind":"mutation","network":"none","local_files":false,"async":{"job_kind":"local_model_download","process":"images.worker.run","terminal_states":["succeeded","failed","cancelled","waiting_input"]}}),json!([{"request":{"member_id":"<已授权成员ID>","operation_key":"<唯一操作标识>"},"prerequisites":["异步入队固定来源与SHA256校验的模型下载任务","下载由images.worker.run真实执行（固定URL，无供应商额度）；本示例不自动执行"]}])),"images.cutout"=>(json!({"kind":"mutation","network":"none","local_files":true,"async":{"job_kind":"local_cutout","process":"images.worker.run","terminal_states":["succeeded","failed","cancelled","waiting_input"]}}),json!([{"request":{"member_id":"<已授权成员ID>","asset_id":"<资产ID>","operation_key":"<唯一操作标识>"},"prerequisites":["创建本地模型抠图任务；本机推理不联网不耗额度","不改变原图或衣物事实；成功生成待采用衍生图"]}])),"images.worker.run"=>(json!({"kind":"mutation","network":"none","local_files":true,"async":null,"conditional_effects":[{"when":{"kind":"local_model_download"},"effect":{"kind":"mutation","network":"model-download"}}]}),json!([{"request":{"member_id":"<已授权成员ID>","job_id":"<local_cutout或local_model_download任务ID>"},"prerequisites":["同步处理持久图片任务：本地抠图离线推理；模型下载按固定来源真实下载并校验SHA256","终态幂等返回任务文档；本示例不自动执行"]}])),_=>unreachable!()};
 let mut properties=json!({"member_id":{"type":"string"}});let mut required=vec!["member_id"];
 if *name=="images.model.download"||*name=="images.cutout"{properties["operation_key"]=json!({"type":"string","minLength":1,"maxLength":120});required.push("operation_key");}
 if *name=="images.cutout"{properties["asset_id"]=json!({"type":"string"});required.push("asset_id");}
 if *name=="images.worker.run"{properties["job_id"]=json!({"type":"string"});required.push("job_id");}
 json!({"name":name,"effect":effect,"examples":examples,"description":match *name{"images.model.status"=>"本地抠图模型下载、校验与离线状态","images.model.download"=>"创建固定来源和SHA256校验的本地模型下载任务","images.cutout"=>"创建真实本地模型抠图任务，不改变原图或衣物事实",_=>"处理持久图片任务，成功生成待采用衍生图"},"inputSchema":{"type":"object","required":required,"properties":properties,"additionalProperties":false},"outputSchema":if *name=="images.model.status"{model.clone()}else{job.clone()}})
 }).collect()
}
