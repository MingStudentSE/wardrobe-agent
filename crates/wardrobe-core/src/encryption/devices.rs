use super::*;
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use rusqlite::{params, OptionalExtension};
use x25519_dalek::{PublicKey, StaticSecret};
use zeroize::Zeroize;
mod package;
struct Secret(Value);
impl Drop for Secret {
    fn drop(&mut self) {
        fn wipe(v: &mut Value) {
            match v {
                Value::String(s) => s.zeroize(),
                Value::Array(a) => a.iter_mut().for_each(wipe),
                Value::Object(o) => o.values_mut().for_each(wipe),
                _ => {}
            }
        }
        wipe(&mut self.0);
    }
}
fn entry(id: &str) -> Result<keyring::Entry, ApiError> {
    credential(id)
}
fn state(app: &Application) -> Result<Option<Value>, ApiError> {
    let s: Option<String> = app
        .connection
        .query_row(
            "SELECT document FROM encryption_state WHERE id=1",
            [],
            |r| r.get(0),
        )
        .optional()?;
    s.map(|s| serde_json::from_str(&s).map_err(|_| invalid()))
        .transpose()
}
fn unlock(s: &Value) -> Result<Zeroizing<[u8; 32]>, ApiError> {
    let encoded = Zeroizing::new(
        entry(string(s, "credential_id")?)?
            .get_password()
            .map_err(|_| ApiError::new("credential_missing", "空间密钥不可用，请使用恢复包"))?,
    );
    let key = Zeroizing::new(STANDARD.decode(encoded.as_bytes()).map_err(|_| invalid())?);
    Ok(Zeroizing::new(
        key.as_slice().try_into().map_err(|_| invalid())?,
    ))
}
fn wallet_kind(s: &Value) -> Result<String, ApiError> {
    Ok(format!(
        "local-wallet:{}:{}",
        string(s, "credential_id")?,
        string(s, "device_id")?
    ))
}
fn wallet(s: &Value) -> Result<Secret, ApiError> {
    let key = unlock(s)?;
    let serialized = open(
        key.as_ref(),
        string(s, "space_id")?,
        0,
        &wallet_kind(s)?,
        &s["wallet_wrap"],
    )?;
    Ok(Secret(
        serde_json::from_slice(&serialized).map_err(|_| invalid())?,
    ))
}
fn wrapped_wallet(s: &Value, w: &Secret, key: &[u8]) -> Result<Value, ApiError> {
    let serialized = Zeroizing::new(w.0.to_string());
    let mut stored = s.clone();
    stored["wallet_wrap"] = seal(
        key,
        string(s, "space_id")?,
        0,
        &wallet_kind(s)?,
        serialized.as_bytes(),
    )?;
    Ok(stored)
}
fn master(w: &Secret, epoch: u64) -> Result<Zeroizing<[u8; 32]>, ApiError> {
    let k = w.0["keys"]
        .as_array()
        .ok_or_else(invalid)?
        .iter()
        .find(|k| k["epoch"] == epoch)
        .ok_or_else(error)?;
    Ok(Zeroizing::new(exact::<32>(k, "key")?))
}
fn existing(app: &Application) -> Result<(Value, Secret), ApiError> {
    let s = state(app)?
        .ok_or_else(|| ApiError::new("encryption_disabled", "请先建立或恢复加密空间"))?;
    let w = wallet(&s)?;
    Ok((s, w))
}
fn update(tx: &rusqlite::Transaction<'_>, s: &Value, w: &Secret) -> Result<(), ApiError> {
    let key = unlock(s)?;
    let stored = wrapped_wallet(s, w, key.as_ref())?;
    tx.execute(
        "UPDATE encryption_state SET document=?1 WHERE id=1",
        [stored.to_string()],
    )?;
    Ok(())
}
fn install(app: &Application, s: &Value, w: &Secret) -> Result<(), ApiError> {
    let tx = app.begin_write()?;
    let exists: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM encryption_state)", [], |r| {
        r.get(0)
    })?;
    if exists {
        return Err(ApiError::new("already_enabled", "此设备已有加密空间"));
    }
    let localspace = app.default_space()?["space_id"].clone();
    if localspace != s["space_id"] {
        let facts:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM clothing) OR EXISTS(SELECT 1 FROM outfits) OR EXISTS(SELECT 1 FROM import_batches) OR EXISTS(SELECT 1 FROM wear_events) OR EXISTS(SELECT 1 FROM assets)",[],|r|r.get(0))?;
        if facts {
            return Err(ApiError::new(
                "space_not_empty",
                "第二设备已有衣橱事实，请先备份并选择空空间进行配对",
            ));
        }
        tx.execute("UPDATE workspace SET id=?1", [string(s, "space_id")?])?;
    }
    let mut stored = s.clone();
    let credential_id = uuid::Uuid::now_v7().to_string();
    stored["credential_id"] = json!(credential_id);
    let key = Zeroizing::new(random::<32>()?);
    let stored = wrapped_wallet(&stored, w, key.as_ref())?;
    let entry = entry(&credential_id)?;
    entry
        .set_password(&STANDARD.encode(key.as_ref()))
        .map_err(|_| ApiError::new("credential_store_unavailable", "无法保存系统本机解锁密钥"))?;
    let result = (|| {
        tx.execute(
            "INSERT INTO encryption_state VALUES(1,?1)",
            [stored.to_string()],
        )?;
        tx.commit()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = entry.delete_credential();
    }
    result
}
fn identity(s: &Value) -> Value {
    let mut i = s.clone();
    if let Some(o) = i.as_object_mut() {
        o.remove("bundle");
        o.remove("manifest");
        o.remove("credential_id");
        o.remove("wallet_wrap");
    }
    i
}
fn validate_wallet(w: &Secret, b: &Value) -> Result<(), ApiError> {
    let i = &w.0["identity"];
    if i["space_id"] != b["space_id"] || i["epoch"] != b["epoch"] {
        return Err(error());
    }
    uuid::Uuid::parse_str(string(i, "device_id")?).map_err(|_| invalid())?;
    let public = PublicKey::from(&StaticSecret::from(exact::<32>(&w.0, "device_secret")?));
    if STANDARD.encode(public.as_bytes()) != string(i, "device_public")? {
        return Err(error());
    }
    if i["owner"] == true {
        let signing = SigningKey::from_bytes(&exact::<32>(&w.0, "owner_secret")?);
        if STANDARD.encode(signing.verifying_key().to_bytes()) != string(i, "owner_public")? {
            return Err(error());
        }
    }
    let keys = w.0["keys"].as_array().ok_or_else(invalid)?;
    let mut seen = std::collections::HashSet::new();
    for k in keys {
        exact::<32>(k, "key")?;
        let e = k["epoch"]
            .as_u64()
            .filter(|e| *e > 0 && *e <= b["epoch"].as_u64().unwrap_or(0))
            .ok_or_else(invalid)?;
        if !seen.insert(e) {
            return Err(invalid());
        }
    }
    if i["pending"] != true && !seen.contains(&b["epoch"].as_u64().ok_or_else(invalid)?) {
        return Err(invalid());
    }
    Ok(())
}
fn status(mut s: Option<Value>) -> Value {
    if let Some(Value::Object(o)) = s.as_mut() {
        o.remove("credential_id");
        o.remove("wallet_wrap");
    }
    json!({"schema_version":1,"enabled":s.is_some(),"state":s})
}
fn device_name(v: &Value) -> Result<&str, ApiError> {
    let name = string(v, "device_name")?.trim();
    if name.is_empty() || name.chars().count() > 80 {
        return Err(ApiError::new("invalid_input", "设备名称须为1–80字符"));
    }
    Ok(name)
}
fn pair_kind(i: &Value) -> Result<String, ApiError> {
    Ok(json!([
        "pair",
        i["invitation_id"],
        i["device_id"],
        i["device_name"],
        i["expires_at"]
    ])
    .to_string())
}
fn dh(secret: &[u8; 32], public: &[u8; 32]) -> Result<Zeroizing<[u8; 32]>, ApiError> {
    let shared = StaticSecret::from(*secret).diffie_hellman(&PublicKey::from(*public));
    if !shared.was_contributory() {
        return Err(error());
    }
    Ok(Zeroizing::new(*shared.as_bytes()))
}
fn signed_manifest(s: &Value, w: &Secret) -> Result<Value, ApiError> {
    let space = string(s, "space_id")?;
    let epoch = s["epoch"].as_u64().ok_or_else(invalid)?;
    let revision = s["revision"].as_u64().ok_or_else(invalid)?;
    let mut recipients = Vec::new();
    for d in s["devices"].as_array().ok_or_else(invalid)? {
        let ephemeral = Zeroizing::new(random::<32>()?);
        let public = PublicKey::from(&StaticSecret::from(*ephemeral));
        let shared = dh(&ephemeral, &exact::<32>(d, "public_key")?)?;
        let kind = format!("device-keys:{}:{revision}", string(d, "device_id")?);
        let key = derive(shared.as_ref(), space, epoch, &kind)?;
        let keys = Zeroizing::new(w.0["keys"].to_string());
        recipients.push(json!({"device_id":d["device_id"],"ephemeral_public":STANDARD.encode(public.as_bytes()),"envelope":seal(key.as_ref(),space,epoch,&kind,keys.as_bytes())?}));
    }
    let payload = json!({"protocol_version":1,"space_id":space,"epoch":epoch,"revision":revision,"owner_public":s["owner_public"],"devices":s["devices"],"recipients":recipients});
    let signing = SigningKey::from_bytes(&exact::<32>(&w.0, "owner_secret")?);
    let encoded = payload.to_string();
    let signature = signing.sign(encoded.as_bytes());
    Ok(
        json!({"payload_base64":STANDARD.encode(encoded.as_bytes()),"signature":STANDARD.encode(signature.to_bytes())}),
    )
}
fn assert_owner(s: &Value) -> Result<(), ApiError> {
    if s["owner"] != true {
        return Err(ApiError::new(
            "owner_required",
            "由空间创建设备管理配对与撤销",
        ));
    }
    Ok(())
}
pub(super) fn execute(app: &Application, op: &str, input: Value) -> Result<Value, ApiError> {
    match op {
        "encryption.package.export" | "encryption.package.read" => package::execute(app, op, input),
        "encryption.status" => {
            fields(&input, &[])?;
            Ok(status(state(app)?))
        }
        "encryption.create" => {
            fields(&input, &["password", "device_name"])?;
            let space = app.default_space()?["space_id"]
                .as_str()
                .unwrap()
                .to_owned();
            let device = uuid::Uuid::now_v7().to_string();
            let secret = Zeroizing::new(random::<32>()?);
            let public = PublicKey::from(&StaticSecret::from(*secret));
            let signing = SigningKey::from_bytes(&random::<32>()?);
            let master = Zeroizing::new(random::<32>()?);
            let mut s = json!({"protocol_version":1,"space_id":space,"device_id":device,"device_name":device_name(&input)?,"epoch":1,"revision":1,"owner":true,"owner_public":STANDARD.encode(signing.verifying_key().to_bytes()),"device_public":STANDARD.encode(public.as_bytes()),"pending":false,"devices":[{"device_id":device,"name":device_name(&input)?,"public_key":STANDARD.encode(public.as_bytes())}]});
            let w = Secret(
                json!({"identity":s,"device_secret":STANDARD.encode(secret.as_ref()),"owner_secret":STANDARD.encode(signing.to_bytes()),"keys":[{"epoch":1,"key":STANDARD.encode(master.as_ref())}]}),
            );
            let serialized = Zeroizing::new(w.0.to_string());
            let (b, recovery) = bundle(
                &space,
                1,
                serialized.as_bytes(),
                string(&input, "password")?,
            )?;
            s["bundle"] = b.clone();
            install(app, &s, &w)?;
            Ok(
                json!({"schema_version":1,"status":status(Some(s)),"recovery_bundle":b,"recovery_secret":recovery}),
            )
        }
        "encryption.encrypt" | "encryption.decrypt" => {
            fields(
                &input,
                if op == "encryption.encrypt" {
                    &["kind", "plaintext_base64"]
                } else {
                    &["kind", "envelope"]
                },
            )?;
            let (s, w) = existing(app)?;
            if s["pending"] == true {
                return Err(ApiError::new(
                    "pairing_pending",
                    "请在创建设备确认配对，再导入确认包",
                ));
            }
            if s["revoked"] == true {
                return Err(ApiError::new("device_revoked", "设备已撤销"));
            }
            let current = s["epoch"].as_u64().ok_or_else(invalid)?;
            let epoch = if op == "encryption.decrypt" {
                input["envelope"]["epoch"]
                    .as_u64()
                    .filter(|e| *e <= current)
                    .ok_or_else(error)?
            } else {
                current
            };
            let space = string(&s, "space_id")?;
            let kind = string(&input, "kind")?;
            if !["changes", "blobs", "snapshots", "device-state"].contains(&kind) {
                return Err(invalid());
            }
            let m = master(&w, epoch)?;
            let key = derive(m.as_ref(), space, epoch, kind)?;
            if op == "encryption.encrypt" {
                let plain = Zeroizing::new(bytes(&input, "plaintext_base64", MAX_BYTES)?);
                Ok(
                    json!({"schema_version":1,"envelope":seal(key.as_ref(),space,epoch,kind,&plain)?}),
                )
            } else {
                let plain = open(key.as_ref(), space, epoch, kind, &input["envelope"])?;
                Ok(json!({"schema_version":1,"plaintext_base64":STANDARD.encode(plain.as_slice())}))
            }
        }
        "encryption.unpair" => {
            fields(&input, &[])?;
            let tx = app.begin_write()?;
            let s =
                state(app)?.ok_or_else(|| ApiError::new("encryption_disabled", "设备未配对"))?;
            match entry(string(&s, "credential_id")?)?.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => {}
                Err(_) => {
                    return Err(ApiError::new(
                        "credential_store_unavailable",
                        "无法移除系统空间密钥",
                    ))
                }
            };
            tx.execute("DELETE FROM encryption_state", [])?;
            tx.execute("DELETE FROM encryption_pairings", [])?;
            tx.commit()?;
            Ok(status(None))
        }
        "encryption.pair.offer" => {
            fields(&input, &["device_name"])?;
            let (s, w) = existing(app)?;
            assert_owner(&s)?;
            let secret = Zeroizing::new(random::<32>()?);
            let id = uuid::Uuid::now_v7().to_string();
            let device = uuid::Uuid::now_v7().to_string();
            let expiry = (chrono::Utc::now() + chrono::Duration::minutes(10)).to_rfc3339();
            let epoch = s["epoch"].as_u64().ok_or_else(invalid)?;
            let space = string(&s, "space_id")?;
            let mut invitation = json!({"protocol_version":1,"invitation_id":id,"space_id":space,"device_id":device,"device_name":device_name(&input)?,"epoch":epoch,"expires_at":expiry});
            let bootstrap = json!({"owner_public":s["owner_public"]});
            invitation["wrapped_key"] = seal(
                secret.as_ref(),
                space,
                epoch,
                &pair_kind(&invitation)?,
                bootstrap.to_string().as_bytes(),
            )?;
            let m = master(&w, epoch)?;
            let record = json!({"invitation":invitation,"consumed":false,"secret_wrap":seal(m.as_ref(),space,epoch,&format!("local-invitation:{id}"),secret.as_ref())?});
            let tx = app.begin_write()?;
            tx.execute(
                "INSERT INTO encryption_pairings VALUES(?1,?2)",
                params![id, record.to_string()],
            )?;
            tx.commit()?;
            Ok(
                json!({"schema_version":1,"invitation":invitation,"pairing_secret":STANDARD.encode(secret.as_ref())}),
            )
        }
        "encryption.pair.accept" => {
            fields(&input, &["invitation", "pairing_secret", "password"])?;
            let i = &input["invitation"];
            fields(
                i,
                &[
                    "protocol_version",
                    "invitation_id",
                    "space_id",
                    "device_id",
                    "device_name",
                    "epoch",
                    "expires_at",
                    "wrapped_key",
                ],
            )?;
            if i["protocol_version"] != 1 {
                return Err(invalid());
            }
            let expiry = chrono::DateTime::parse_from_rfc3339(string(i, "expires_at")?)
                .map_err(|_| invalid())?;
            if expiry.with_timezone(&chrono::Utc) <= chrono::Utc::now() {
                return Err(ApiError::new("invitation_expired", "配对邀请已到期"));
            }
            let space = string(i, "space_id")?;
            let device = string(i, "device_id")?;
            uuid::Uuid::parse_str(space).map_err(|_| invalid())?;
            uuid::Uuid::parse_str(device).map_err(|_| invalid())?;
            let epoch = i["epoch"].as_u64().filter(|e| *e > 0).ok_or_else(invalid)?;
            let secret = Zeroizing::new(exact::<32>(&input, "pairing_secret")?);
            let bootstrap = open(
                secret.as_ref(),
                space,
                epoch,
                &pair_kind(i)?,
                &i["wrapped_key"],
            )?;
            let identity: Value = serde_json::from_slice(&bootstrap).map_err(|_| invalid())?;
            exact::<32>(&identity, "owner_public")?;
            let device_secret = Zeroizing::new(random::<32>()?);
            let device_public = PublicKey::from(&StaticSecret::from(*device_secret));
            let mut s = json!({"protocol_version":1,"space_id":space,"device_id":device,"device_name":i["device_name"],"epoch":epoch,"revision":0,"owner":false,"owner_public":identity["owner_public"],"device_public":STANDARD.encode(device_public.as_bytes()),"pending":true,"devices":[]});
            let w = Secret(
                json!({"identity":s,"device_secret":STANDARD.encode(device_secret.as_ref()),"keys":[]}),
            );
            let serialized = Zeroizing::new(w.0.to_string());
            let (b, recovery) = bundle(
                space,
                epoch,
                serialized.as_bytes(),
                string(&input, "password")?,
            )?;
            s["bundle"] = b.clone();
            let receiptbody =
                json!({"device_id":device,"public_key":STANDARD.encode(device_public.as_bytes())});
            let receipt = json!({"invitation_id":i["invitation_id"],"envelope":seal(secret.as_ref(),space,epoch,&format!("pair-receipt:{}",string(i,"invitation_id")?),receiptbody.to_string().as_bytes())?});
            install(app, &s, &w)?;
            Ok(
                json!({"schema_version":1,"state":s,"receipt":receipt,"recovery_bundle":b,"recovery_secret":recovery}),
            )
        }
        "encryption.pair.confirm" => {
            fields(&input, &["receipt"])?;
            let tx = app.begin_write()?;
            let (mut s, w) = existing(app)?;
            assert_owner(&s)?;
            let receipt = &input["receipt"];
            fields(receipt, &["invitation_id", "envelope"])?;
            let id = string(receipt, "invitation_id")?;
            let record: String = tx
                .query_row(
                    "SELECT document FROM encryption_pairings WHERE id=?1",
                    [id],
                    |r| r.get(0),
                )
                .optional()?
                .ok_or_else(|| ApiError::new("not_found", "配对邀请不存在"))?;
            let mut record: Value = serde_json::from_str(&record).map_err(|_| invalid())?;
            if record["consumed"] == true {
                return Err(ApiError::new("invitation_consumed", "配对邀请已使用"));
            }
            let i = &record["invitation"];
            if chrono::DateTime::parse_from_rfc3339(string(i, "expires_at")?)
                .map_err(|_| invalid())?
                .with_timezone(&chrono::Utc)
                <= chrono::Utc::now()
            {
                return Err(ApiError::new("invitation_expired", "配对邀请已到期"));
            }
            let epoch = s["epoch"].as_u64().ok_or_else(invalid)?;
            if i["epoch"] != epoch {
                return Err(ApiError::new(
                    "invitation_expired",
                    "密钥已轮换，请重新配对",
                ));
            }
            let space = string(&s, "space_id")?.to_owned();
            let m = master(&w, epoch)?;
            let secret = open(
                m.as_ref(),
                &space,
                epoch,
                &format!("local-invitation:{id}"),
                &record["secret_wrap"],
            )?;
            let body = open(
                &secret,
                &space,
                epoch,
                &format!("pair-receipt:{id}"),
                &receipt["envelope"],
            )?;
            let body: Value = serde_json::from_slice(&body).map_err(|_| invalid())?;
            if body["device_id"] != i["device_id"] {
                return Err(error());
            }
            exact::<32>(&body, "public_key")?;
            s["devices"].as_array_mut().ok_or_else(invalid)?.push(json!({"device_id":body["device_id"],"name":i["device_name"],"public_key":body["public_key"]}));
            s["revision"] = json!(s["revision"]
                .as_u64()
                .ok_or_else(invalid)?
                .checked_add(1)
                .ok_or_else(invalid)?);
            let manifest = signed_manifest(&s, &w)?;
            s["manifest"] = manifest.clone();
            record["consumed"] = json!(true);
            tx.execute(
                "UPDATE encryption_pairings SET document=?1 WHERE id=?2",
                params![record.to_string(), id],
            )?;
            tx.execute(
                "UPDATE encryption_state SET document=?1 WHERE id=1",
                [s.to_string()],
            )?;
            tx.commit()?;
            Ok(json!({"schema_version":1,"manifest":manifest}))
        }
        "encryption.device.apply" => {
            let tx = app.begin_write()?;
            fields(&input, &["manifest"])?;
            let (mut s, mut w) = existing(app)?;
            let manifest = &input["manifest"];
            fields(manifest, &["payload_base64", "signature"])?;
            let payload_bytes = bytes(manifest, "payload_base64", MAX_BYTES)?;
            let verifier =
                VerifyingKey::from_bytes(&exact::<32>(&s, "owner_public")?).map_err(|_| error())?;
            verifier
                .verify_strict(
                    &payload_bytes,
                    &Signature::from_bytes(&exact::<64>(manifest, "signature")?),
                )
                .map_err(|_| error())?;
            let p: Value = serde_json::from_slice(&payload_bytes).map_err(|_| invalid())?;
            fields(
                &p,
                &[
                    "protocol_version",
                    "space_id",
                    "epoch",
                    "revision",
                    "owner_public",
                    "devices",
                    "recipients",
                ],
            )?;
            if p["protocol_version"] != 1
                || p["space_id"] != s["space_id"]
                || p["owner_public"] != s["owner_public"]
            {
                return Err(error());
            }
            let revision = p["revision"].as_u64().ok_or_else(invalid)?;
            if revision <= s["revision"].as_u64().ok_or_else(invalid)? {
                if s["manifest"] == *manifest {
                    return Ok(status(Some(s)));
                }
                return Err(ApiError::new("stale_device_state", "拒绝过期设备状态"));
            }
            let epoch = p["epoch"]
                .as_u64()
                .filter(|e| *e >= s["epoch"].as_u64().unwrap_or(1))
                .ok_or_else(error)?;
            let id = string(&s, "device_id")?.to_owned();
            let recipient = p["recipients"]
                .as_array()
                .ok_or_else(invalid)?
                .iter()
                .find(|r| r["device_id"] == id);
            if recipient.is_none() {
                s["revoked"] = json!(true);
                s["revision"] = json!(revision);
                update(&tx, &s, &w)?;
                tx.commit()?;
                return Err(ApiError::new(
                    "device_revoked",
                    "设备已撤销，无法取得新密钥",
                ));
            }
            let recipient = recipient.unwrap();
            let device = p["devices"]
                .as_array()
                .ok_or_else(invalid)?
                .iter()
                .find(|d| d["device_id"] == id)
                .ok_or_else(error)?;
            if device["public_key"] != s["device_public"] {
                return Err(error());
            }
            let shared = dh(
                &exact::<32>(&w.0, "device_secret")?,
                &exact::<32>(recipient, "ephemeral_public")?,
            )?;
            let kind = format!("device-keys:{id}:{revision}");
            let key = derive(shared.as_ref(), string(&s, "space_id")?, epoch, &kind)?;
            let decrypted = open(
                key.as_ref(),
                string(&s, "space_id")?,
                epoch,
                &kind,
                &recipient["envelope"],
            )?;
            let keys: Value = serde_json::from_slice(&decrypted).map_err(|_| invalid())?;
            if !keys
                .as_array()
                .is_some_and(|a| a.iter().any(|k| k["epoch"] == epoch))
            {
                return Err(invalid());
            }
            for k in keys.as_array().unwrap() {
                exact::<32>(k, "key")?;
            }
            w.0["keys"] = keys;
            s["epoch"] = json!(epoch);
            s["revision"] = json!(revision);
            s["devices"] = p["devices"].clone();
            s["manifest"] = manifest.clone();
            s["pending"] = json!(false);
            w.0["identity"] = identity(&s);
            update(&tx, &s, &w)?;
            tx.commit()?;
            Ok(status(Some(s)))
        }
        "encryption.restore" => {
            fields(&input, &["bundle", "password", "recovery_secret"])?;
            if input["password"].is_string() == input["recovery_secret"].is_string() {
                return Err(ApiError::new("invalid_input", "请选择密码或恢复密钥之一"));
            }
            let b = &input["bundle"];
            let decrypted = recover(
                b,
                input["password"].as_str(),
                input["recovery_secret"].as_str(),
            )?;
            let w = Secret(serde_json::from_slice(&decrypted).map_err(|_| invalid())?);
            validate_wallet(&w, b)?;
            let mut s = w.0["identity"].clone();
            s["bundle"] = b.clone();
            install(app, &s, &w)?;
            Ok(status(Some(s)))
        }
        "encryption.password.change" => {
            let tx = app.begin_write()?;
            fields(&input, &["old_password", "new_password"])?;
            let (mut s, mut w) = existing(app)?;
            let old = recover(&s["bundle"], Some(string(&input, "old_password")?), None)?;
            let old = Secret(serde_json::from_slice(&old).map_err(|_| invalid())?);
            if old.0["identity"]["device_id"] != s["device_id"]
                || old.0["device_secret"] != w.0["device_secret"]
            {
                return Err(error());
            }
            w.0["identity"] = identity(&s);
            let serialized = Zeroizing::new(w.0.to_string());
            let (b, recovery) = bundle(
                string(&s, "space_id")?,
                s["epoch"].as_u64().ok_or_else(invalid)?,
                serialized.as_bytes(),
                string(&input, "new_password")?,
            )?;
            s["bundle"] = b.clone();
            update(&tx, &s, &w)?;
            tx.commit()?;
            Ok(
                json!({"schema_version":1,"recovery_bundle":b,"recovery_secret":recovery,"status":status(Some(s))}),
            )
        }
        "encryption.device.revoke" => {
            let tx = app.begin_write()?;
            fields(&input, &["device_id", "password"])?;
            let (mut s, mut w) = existing(app)?;
            assert_owner(&s)?;
            let id = string(&input, "device_id")?;
            if s["device_id"] == id {
                return Err(ApiError::new(
                    "invalid_input",
                    "创建设备请使用解除配对；撤销用于其他设备",
                ));
            }
            let devices = s["devices"].as_array_mut().ok_or_else(invalid)?;
            if !devices.iter().any(|d| d["device_id"] == id) {
                return Err(ApiError::new("not_found", "配对设备不存在"));
            }
            devices.retain(|d| d["device_id"] != id);
            let epoch = s["epoch"]
                .as_u64()
                .ok_or_else(invalid)?
                .checked_add(1)
                .ok_or_else(invalid)?;
            s["epoch"] = json!(epoch);
            s["revision"] = json!(s["revision"]
                .as_u64()
                .ok_or_else(invalid)?
                .checked_add(1)
                .ok_or_else(invalid)?);
            let key = Zeroizing::new(random::<32>()?);
            w.0["keys"]
                .as_array_mut()
                .ok_or_else(invalid)?
                .push(json!({"epoch":epoch,"key":STANDARD.encode(key.as_ref())}));
            w.0["identity"] = identity(&s);
            let serialized = Zeroizing::new(w.0.to_string());
            let (b, recovery) = bundle(
                string(&s, "space_id")?,
                epoch,
                serialized.as_bytes(),
                string(&input, "password")?,
            )?;
            s["bundle"] = b.clone();
            let manifest = signed_manifest(&s, &w)?;
            s["manifest"] = manifest.clone();
            update(&tx, &s, &w)?;
            tx.commit()?;
            Ok(
                json!({"schema_version":1,"manifest":manifest,"recovery_bundle":b,"recovery_secret":recovery}),
            )
        }
        _ => Err(ApiError::new("unsupported_operation", "此加密操作尚不可用")),
    }
}
