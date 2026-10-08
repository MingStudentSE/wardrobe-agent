use crate::{string, ApiError, Application};
use argon2::{Algorithm, Argon2, Params, Version};
use base64::{engine::general_purpose::STANDARD, Engine};
use chacha20poly1305::{
    aead::{Aead, Payload},
    KeyInit, XChaCha20Poly1305, XNonce,
};
use hkdf::Hkdf;
use rusqlite::Transaction;
use serde_json::{json, Value};
use sha2::Sha256;
use zeroize::Zeroizing;
const MAX_BYTES: usize = 16 * 1024 * 1024;
fn error() -> ApiError {
    ApiError::new("authentication_failed", "密钥或认证数据不匹配，未返回明文")
}
fn invalid() -> ApiError {
    ApiError::new("invalid_envelope", "加密格式、版本或字节长度无效")
}
fn random<const N: usize>() -> Result<[u8; N], ApiError> {
    let mut bytes = [0u8; N];
    getrandom::fill(&mut bytes).map_err(|_| ApiError::new("random_error", "系统随机源不可用"))?;
    Ok(bytes)
}
fn bytes(v: &Value, name: &str, max: usize) -> Result<Vec<u8>, ApiError> {
    let text = string(v, name)?;
    if text.len() > max.saturating_add(2) / 3 * 4 + 4 {
        return Err(invalid());
    }
    let b = STANDARD.decode(text).map_err(|_| invalid())?;
    if b.len() > max {
        return Err(invalid());
    }
    Ok(b)
}
fn exact<const N: usize>(v: &Value, name: &str) -> Result<[u8; N], ApiError> {
    bytes(v, name, N)?.try_into().map_err(|_| invalid())
}
fn fields(v: &Value, allowed: &[&str]) -> Result<(), ApiError> {
    if !v
        .as_object()
        .is_some_and(|o| o.keys().all(|k| allowed.contains(&k.as_str())))
    {
        return Err(ApiError::new("invalid_input", "输入包含不支持的字段"));
    }
    Ok(())
}
fn aad(space: &str, epoch: u64, kind: &str) -> Vec<u8> {
    json!(["wardrobe-e2ee", 1, space, epoch, kind])
        .to_string()
        .into_bytes()
}
fn derive(
    master: &[u8],
    space: &str,
    epoch: u64,
    kind: &str,
) -> Result<Zeroizing<[u8; 32]>, ApiError> {
    let mut key = Zeroizing::new([0u8; 32]);
    Hkdf::<Sha256>::new(Some(space.as_bytes()), master)
        .expand(&aad(space, epoch, kind), key.as_mut())
        .map_err(|_| invalid())?;
    Ok(key)
}
fn seal(key: &[u8], space: &str, epoch: u64, kind: &str, plain: &[u8]) -> Result<Value, ApiError> {
    let nonce = random::<24>()?;
    let cipher = XChaCha20Poly1305::new_from_slice(key).map_err(|_| invalid())?;
    let encrypted = cipher
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: plain,
                aad: &aad(space, epoch, kind),
            },
        )
        .map_err(|_| error())?;
    Ok(
        json!({"protocol_version":1,"algorithm":"XChaCha20-Poly1305","space_id":space,"epoch":epoch,"kind":kind,"nonce":STANDARD.encode(nonce),"ciphertext":STANDARD.encode(encrypted)}),
    )
}
fn open(
    key: &[u8],
    space: &str,
    epoch: u64,
    kind: &str,
    envelope: &Value,
) -> Result<Zeroizing<Vec<u8>>, ApiError> {
    fields(
        envelope,
        &[
            "protocol_version",
            "algorithm",
            "space_id",
            "epoch",
            "kind",
            "nonce",
            "ciphertext",
        ],
    )?;
    if envelope["protocol_version"] != 1 || envelope["algorithm"] != "XChaCha20-Poly1305" {
        return Err(invalid());
    }
    if envelope["space_id"] != space || envelope["epoch"] != epoch || envelope["kind"] != kind {
        return Err(error());
    }
    let nonce = exact::<24>(envelope, "nonce")?;
    let ciphertext = bytes(envelope, "ciphertext", MAX_BYTES + 16)?;
    if ciphertext.len() < 16 {
        return Err(invalid());
    }
    let cipher = XChaCha20Poly1305::new_from_slice(key).map_err(|_| invalid())?;
    let clear = cipher
        .decrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: &ciphertext,
                aad: &aad(space, epoch, kind),
            },
        )
        .map_err(|_| error())?;
    Ok(Zeroizing::new(clear))
}
fn password_key(password: &str, salt: &[u8]) -> Result<Zeroizing<[u8; 32]>, ApiError> {
    if password.len() < 12 || password.len() > 1024 {
        return Err(ApiError::new("invalid_input", "空间密码须为12–1024字节"));
    }
    let parameters = Params::new(65536, 3, 1, Some(32)).map_err(|_| invalid())?;
    let mut key = Zeroizing::new([0; 32]);
    Argon2::new(Algorithm::Argon2id, Version::V0x13, parameters)
        .hash_password_into(password.as_bytes(), salt, key.as_mut())
        .map_err(|_| invalid())?;
    Ok(key)
}
fn bundle(
    space: &str,
    epoch: u64,
    master: &[u8],
    password: &str,
) -> Result<(Value, String), ApiError> {
    let salt = random::<16>()?;
    let wrap = password_key(password, &salt)?;
    let recovery = Zeroizing::new(random::<32>()?);
    Ok((
        json!({"protocol_version":1,"space_id":space,"epoch":epoch,"kdf":{"algorithm":"Argon2id","version":19,"memory_kib":65536,"iterations":3,"lanes":1,"salt":STANDARD.encode(salt)},"password_wrap":seal(wrap.as_ref(),space,epoch,"password-wrap",master)?,"recovery_wrap":seal(recovery.as_ref(),space,epoch,"recovery-wrap",master)?}),
        STANDARD.encode(recovery.as_ref()),
    ))
}
fn recover(
    b: &Value,
    password: Option<&str>,
    secret: Option<&str>,
) -> Result<Zeroizing<Vec<u8>>, ApiError> {
    fields(
        b,
        &[
            "protocol_version",
            "space_id",
            "epoch",
            "kdf",
            "password_wrap",
            "recovery_wrap",
        ],
    )?;
    if b["protocol_version"] != 1 {
        return Err(invalid());
    }
    let space = string(b, "space_id")?;
    uuid::Uuid::parse_str(space).map_err(|_| invalid())?;
    let epoch = b["epoch"].as_u64().filter(|e| *e > 0).ok_or_else(invalid)?;
    let key;
    let kind;
    let e;
    if let Some(password) = password {
        let kdf = &b["kdf"];
        fields(
            kdf,
            &[
                "algorithm",
                "version",
                "memory_kib",
                "iterations",
                "lanes",
                "salt",
            ],
        )?;
        if kdf["algorithm"] != "Argon2id"
            || kdf["version"] != 19
            || kdf["memory_kib"] != 65536
            || kdf["iterations"] != 3
            || kdf["lanes"] != 1
        {
            return Err(invalid());
        }
        key = password_key(password, &exact::<16>(kdf, "salt")?)?;
        kind = "password-wrap";
        e = &b["password_wrap"];
    } else {
        key = Zeroizing::new(exact::<32>(
            &json!({"secret":secret.ok_or_else(invalid)?}),
            "secret",
        )?);
        kind = "recovery-wrap";
        e = &b["recovery_wrap"];
    }
    let master = open(key.as_ref(), space, epoch, kind, e)?;
    Ok(master)
}
pub(crate) fn migrate(tx: &Transaction<'_>) -> Result<(), ApiError> {
    tx.execute_batch("CREATE TABLE IF NOT EXISTS encryption_state (id INTEGER PRIMARY KEY CHECK(id=1), document TEXT NOT NULL); CREATE TABLE IF NOT EXISTS encryption_pairings(id TEXT PRIMARY KEY, document TEXT NOT NULL);")?;
    Ok(())
}
fn credential(id: &str) -> Result<keyring::Entry, ApiError> {
    keyring::Entry::new("wardrobe-agent.encryption", id)
        .map_err(|_| ApiError::new("credential_store_unavailable", "系统安全存储不可用"))
}
mod devices;
pub(crate) use devices::{WalletStatus, wallet_status};
impl Application {
    pub(crate) fn encryption_operation(&self, op: &str, input: Value) -> Result<Value, ApiError> {
        match op {
            "encryption.recovery.create" => {
                fields(&input, &["password"])?;
                let space = self.default_space()?["space_id"]
                    .as_str()
                    .unwrap()
                    .to_string();
                let master = Zeroizing::new(random::<32>()?);
                let (b, recovery) =
                    bundle(&space, 1, master.as_ref(), string(&input, "password")?)?;
                Ok(json!({"schema_version":1,"bundle":b,"recovery_secret":recovery}))
            }
            "encryption.seal" | "encryption.open" => {
                fields(
                    &input,
                    if op == "encryption.seal" {
                        &[
                            "bundle",
                            "password",
                            "recovery_secret",
                            "kind",
                            "plaintext_base64",
                        ]
                    } else {
                        &["bundle", "password", "recovery_secret", "kind", "envelope"]
                    },
                )?;
                let b = &input["bundle"];
                let master = recover(
                    b,
                    input["password"].as_str(),
                    input["recovery_secret"].as_str(),
                )?;
                if master.len() != 32 {
                    return Err(invalid());
                }
                let space = string(b, "space_id")?;
                let epoch = b["epoch"].as_u64().ok_or_else(invalid)?;
                let kind = string(&input, "kind")?;
                if !["changes", "blobs", "snapshots", "device-state"].contains(&kind) {
                    return Err(ApiError::new("invalid_input", "不支持的加密用途"));
                }
                let key = derive(master.as_ref(), space, epoch, kind)?;
                if op == "encryption.seal" {
                    let plain = Zeroizing::new(bytes(&input, "plaintext_base64", MAX_BYTES)?);
                    Ok(
                        json!({"schema_version":1,"envelope":seal(key.as_ref(),space,epoch,kind,&plain)?}),
                    )
                } else {
                    let plain = open(key.as_ref(), space, epoch, kind, &input["envelope"])?;
                    Ok(
                        json!({"schema_version":1,"plaintext_base64":STANDARD.encode(plain.as_slice())}),
                    )
                }
            }
            _ => devices::execute(self, op, input),
        }
    }
}
pub(crate) fn descriptors() -> Vec<Value> {
    let operations = [
        (
            "encryption.package.export",
            vec!["directory"],
            "将白名单事实与采用展示图导出为独立密文包",
        ),
        (
            "encryption.package.read",
            vec!["path"],
            "验证并读取密文包，不导入事实",
        ),
        ("encryption.status", vec![], "读取本机加密空间状态"),
        (
            "encryption.create",
            vec!["password", "device_name"],
            "建立加密空间并保存系统密钥",
        ),
        (
            "encryption.restore",
            vec!["bundle"],
            "使用密码或恢复密钥恢复空间",
        ),
        (
            "encryption.password.change",
            vec!["old_password", "new_password"],
            "重新包装空间密钥",
        ),
        (
            "encryption.unpair",
            vec![],
            "解除本机设备配对并移除安全密钥",
        ),
        (
            "encryption.pair.offer",
            vec!["device_name"],
            "发起十分钟单次设备邀请",
        ),
        (
            "encryption.pair.accept",
            vec!["invitation", "pairing_secret", "password"],
            "第二设备接受邀请并生成确认回执",
        ),
        (
            "encryption.pair.confirm",
            vec!["receipt"],
            "创建设备确认第二设备",
        ),
        (
            "encryption.device.apply",
            vec!["manifest"],
            "验证并应用创建设备签名状态",
        ),
        (
            "encryption.device.revoke",
            vec!["device_id", "password"],
            "撤销设备并轮换未来密钥",
        ),
        (
            "encryption.encrypt",
            vec!["kind", "plaintext_base64"],
            "使用系统空间密钥加密字节",
        ),
        (
            "encryption.decrypt",
            vec!["kind", "envelope"],
            "使用系统空间密钥认证解密字节",
        ),
        (
            "encryption.recovery.create",
            vec!["password"],
            "生成独立密码与恢复包装包",
        ),
        (
            "encryption.seal",
            vec!["bundle", "kind", "plaintext_base64"],
            "使用独立恢复包加密字节",
        ),
        (
            "encryption.open",
            vec!["bundle", "kind", "envelope"],
            "使用独立恢复包认证解密字节",
        ),
    ];
    operations.into_iter().map(|(name,required,description)|{
 let mut properties=serde_json::Map::new();for field in ["directory","path","password","device_name","old_password","new_password","pairing_secret","device_id","recovery_secret","kind","plaintext_base64"]{properties.insert(field.into(),json!({"type":"string"}));}for field in ["bundle","invitation","receipt","manifest","envelope"]{properties.insert(field.into(),json!({"type":"object"}));}
 let (effect,examples)=match &name[..] {
            "encryption.status"=>(json!({"kind":"query","network":"none","local_files":false,"async":null}),json!([{"request":{},"prerequisites":["读取本机加密空间状态；只读不改"]}])),"encryption.package.read"=>(json!({"kind":"query","network":"none","local_files":true,"async":null}),json!([{"request":{"path":"</绝对/本地/密文包路径>"},"prerequisites":["验证并读取密文包；认证检查不导入任何事实"]}])),"encryption.package.export"=>(json!({"kind":"mutation","network":"none","local_files":true,"async":null}),json!([{"request":{"directory":"</绝对/本地/导出目录>"},"prerequisites":["将白名单事实与采用展示图导出为独立密文包；原图与私人图片不上包"]}])),"encryption.seal"=>(json!({"kind":"mutation","network":"none","local_files":true,"async":null}),json!([{"request":{"bundle":{"<设备名>":"<恢复包内容对象>"},"kind":"password","plaintext_base64":"<base64明文占位>"},"prerequisites":["使用独立恢复包加密字节；对象字段以inputSchema为准","口令与明文示例均为占位"]}])),"encryption.open"=>(json!({"kind":"mutation","network":"none","local_files":true,"async":null}),json!([{"request":{"bundle":{"<设备名>":"<恢复包内容对象>"},"kind":"password","envelope":{"<封装字段>":"<密文封装对象>"}},"prerequisites":["使用独立恢复包认证解密；对象字段以inputSchema为准","口令类输入走安全输入，示例均为占位"]}])),"encryption.restore"=>(json!({"kind":"mutation","network":"none","local_files":true,"async":null}),json!([{"request":{"bundle":{"<设备名>":"<恢复包内容对象>"},"password":"<密码占位，勿用真实口令>"},"prerequisites":["使用密码或恢复密钥恢复空间；失败不破坏现有数据；口令占位非真实凭据"]}])),_=>(json!({"kind":"mutation","network":"none","local_files":false,"async":null}),{ let request: serde_json::Map<String, serde_json::Value> = required.iter().map(|f| {
            let value = match *f {
                "password" | "old_password" | "new_password" => json!("<密码占位，勿用真实口令>"),
                "device_name" => json!("<设备名>"),
                "directory" => json!("</绝对/本地/目录>"),
                "path" | "bundle" => json!("</绝对/本地/路径>"),
                _ => if properties.get(*f).and_then(|p| p.get("type")).and_then(|t| t.as_str()) == Some("object") { json!({}) } else { json!(*f) },
            };
            (f.to_string(), value)
        }).collect();
        json!([{"request":Value::Object(request),"prerequisites":["本机端到端加密操作；不联网；口令类字段示例均为占位符，真实口令走安全输入"]}])})
        };
        json!({"name":name,"effect":effect,"examples":examples,"description":description,"inputSchema":{"type":"object","required":required,"properties":properties},"outputSchema":if &name[..]=="encryption.status"{json!({"type":"object","required":["schema_version","enabled","state"],"properties":{"schema_version":{"const":1},"enabled":{"type":"boolean"},"state":{}}})}else{json!({"type":"object","required":["schema_version"],"properties":{"schema_version":{"type":"integer","const":1}}})}})
 }).collect()
}
