use super::*;
fn check(valid: bool) -> Result<(), ApiError> {
    if valid {
        Ok(())
    } else {
        Err(error("sync_invalid"))
    }
}
fn fields(doc: &Value, allowed: &[&str]) -> Result<(), ApiError> {
    check(
        doc.as_object()
            .is_some_and(|o| o.keys().all(|k| allowed.contains(&k.as_str()))),
    )
}
fn id(doc: &Value, key: &str) -> Result<(), ApiError> {
    check(
        doc[key]
            .as_str()
            .is_some_and(|s| uuid::Uuid::parse_str(s).is_ok()),
    )
}
fn text(doc: &Value, key: &str, min: usize, max: usize) -> Result<(), ApiError> {
    check(
        doc[key]
            .as_str()
            .is_some_and(|s| (min..=max).contains(&s.chars().count())),
    )
}
fn timestamp(doc: &Value, key: &str) -> Result<(), ApiError> {
    check(
        doc[key]
            .as_str()
            .is_some_and(|s| chrono::DateTime::parse_from_rfc3339(s).is_ok()),
    )
}
fn revision(doc: &Value, key: &str) -> Result<(), ApiError> {
    check(
        doc[key]
            .as_u64()
            .is_some_and(|v| v > 0 && v < i64::MAX as u64),
    )
}
const FACTS: &[&str] = &[
    "sub_category",
    "brand",
    "size_label",
    "fit",
    "description",
    "notes",
    "purchase_date",
    "purchase_store",
    "purchase_url",
    "color_description",
    "color",
    "pattern",
    "material",
    "style",
    "occasion",
    "tags",
    "name",
    "category",
    "seasons",
    "favorite",
    "purchase_price_minor",
    "currency",
];
const CLOTHING: &[&str] = &[
    "schema_version",
    "id",
    "member_id",
    "created_at",
    "updated_at",
    "status",
    "deleted_at",
    "status_before_delete",
    "taxonomy_version",
    "price_exponent",
    "asset_ids",
    "display_asset_id",
    "sub_category",
    "brand",
    "size_label",
    "fit",
    "description",
    "notes",
    "purchase_date",
    "purchase_store",
    "purchase_url",
    "color_description",
    "color",
    "pattern",
    "material",
    "style",
    "occasion",
    "tags",
    "name",
    "category",
    "seasons",
    "favorite",
    "purchase_price_minor",
    "currency",
];
fn clothing(doc: &Value, member: &str) -> Result<(), ApiError> {
    fields(doc, CLOTHING)?;
    id(doc, "id")?;
    check(doc["member_id"] == member && doc["schema_version"] == 1)?;
    check(
        ["active", "archived", "deleted"]
            .iter()
            .any(|s| doc["status"] == *s),
    )?;
    if !doc["deleted_at"].is_null() {
        timestamp(doc, "deleted_at")?;
    }
    if !doc["status_before_delete"].is_null() {
        check(
            ["active", "archived"]
                .iter()
                .any(|s| doc["status_before_delete"] == *s),
        )?;
    }
    text(doc, "name", 1, 120)?;
    timestamp(doc, "created_at")?;
    timestamp(doc, "updated_at")?;
    let input: serde_json::Map<String, Value> = doc
        .as_object()
        .unwrap()
        .iter()
        .filter(|(k, _)| FACTS.contains(&k.as_str()))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    let mut validated = doc.clone();
    crate::clothing::apply_facts(&mut validated, &Value::Object(input))
        .map_err(|_| error("sync_invalid"))?;
    check(validated["price_exponent"] == doc["price_exponent"])?;
    if let Some(ids) = doc.get("asset_ids") {
        let ids = ids.as_array().ok_or_else(|| error("sync_invalid"))?;
        check(ids.len() <= 20)?;
        for value in ids {
            check(
                value
                    .as_str()
                    .is_some_and(|s| uuid::Uuid::parse_str(s).is_ok()),
            )?;
        }
    }
    if !doc["display_asset_id"].is_null() {
        id(doc, "display_asset_id")?;
    }
    Ok(())
}
fn items(doc: &Value, member: &str, outfit: bool) -> Result<(), ApiError> {
    let items = doc["items"]
        .as_array()
        .ok_or_else(|| error("sync_invalid"))?;
    check(!items.is_empty() && items.len() <= 100)?;
    let mut seen = std::collections::BTreeSet::new();
    for item in items {
        fields(
            item,
            if outfit {
                &["clothing_id", "role", "layer", "snapshot"]
            } else {
                &["clothing_id", "role", "layer", "snapshot"]
            },
        )?;
        id(item, "clothing_id")?;
        check(seen.insert(string(item, "clothing_id")?.to_owned()))?;
        if outfit || item.get("role").is_some() {
            text(item, "role", 1, 40)?;
            check(
                item["layer"]
                    .as_i64()
                    .is_some_and(|v| (-100..=100).contains(&v)),
            )?;
        }
        clothing(&item["snapshot"], member)?;
        check(item["snapshot"]["id"] == item["clothing_id"])?;
    }
    Ok(())
}
pub(super) fn validate(table: &str, entity: &str, doc: &Value) -> Result<(), ApiError> {
    if table == "members" {
        fields(doc, &["id", "name"])?;
        id(doc, "id")?;
        text(doc, "name", 1, 80)?;
        return check(doc["id"] == entity);
    }
    id(doc, "member_id")?;
    check(doc["schema_version"] == 1)?;
    let member = string(doc, "member_id")?;
    match table {
        "clothing" => {
            clothing(doc, member)?;
            check(doc["id"] == entity)?;
        }
        "outfits" | "outfit_versions" => {
            fields(
                doc,
                &[
                    "schema_version",
                    "id",
                    "member_id",
                    "name",
                    "version",
                    "status",
                    "favorite",
                    "items",
                    "created_at",
                    "updated_at",
                ],
            )?;
            id(doc, "id")?;
            text(doc, "name", 1, 120)?;
            revision(doc, "version")?;
            timestamp(doc, "created_at")?;
            timestamp(doc, "updated_at")?;
            check(
                doc["favorite"].is_boolean()
                    && ["active", "deleted"].iter().any(|s| doc["status"] == *s),
            )?;
            items(doc, member, true)?;
            check(if table == "outfits" {
                doc["id"] == entity
            } else {
                entity
                    == format!(
                        "{}:{}",
                        string(doc, "id")?,
                        doc["version"].as_u64().unwrap()
                    )
            })?;
        }
        "wear_events" | "care_events" => {
            fields(
                doc,
                &[
                    "schema_version",
                    "id",
                    "member_id",
                    "root_event_id",
                    "revision",
                    "kind",
                    "supersedes_event_id",
                    "items",
                    "source_outfit",
                    "notes",
                    "created_at",
                    "updated_at",
                    "local_date",
                    "timezone",
                    "local_time",
                    "time_precision",
                    "occurred_at",
                    "reason",
                    "clothing_id",
                    "snapshot",
                    "action",
                ],
            )?;
            id(doc, "id")?;
            id(doc, "root_event_id")?;
            check(doc["id"] == entity)?;
            revision(doc, "revision")?;
            timestamp(doc, "created_at")?;
            text(doc, "notes", 0, 2000)?;
            check(
                ["record", "correction", "revocation"]
                    .iter()
                    .any(|s| doc["kind"] == *s),
            )?;
            let rev = doc["revision"].as_u64().unwrap();
            if rev == 1 {
                check(
                    doc["kind"] == "record"
                        && doc["root_event_id"] == doc["id"]
                        && doc["supersedes_event_id"].is_null(),
                )?;
            } else {
                id(doc, "supersedes_event_id")?;
                check(doc["kind"] != "record")?;
            }
            let mut timing = json!({"local_date":doc["local_date"],"timezone":doc["timezone"]});
            if !doc["local_time"].is_null() {
                timing["local_time"] = doc["local_time"].clone();
            }
            let computed = crate::wears::timing(&timing).map_err(|_| error("sync_invalid"))?;
            for key in [
                "local_date",
                "timezone",
                "local_time",
                "time_precision",
                "occurred_at",
            ] {
                check(computed[key] == doc[key])?;
            }
            if doc["kind"] == "revocation" {
                text(doc, "reason", 1, 2000)?;
            }
            if table == "wear_events" {
                items(doc, member, false)?;
                if !doc["source_outfit"].is_null() {
                    let source = &doc["source_outfit"];
                    check(source["member_id"] == member)?;
                    validate("outfits", string(source, "id")?, source)?;
                }
            } else {
                id(doc, "clothing_id")?;
                clothing(&doc["snapshot"], member)?;
                check(doc["snapshot"]["id"] == doc["clothing_id"])?;
                check(
                    [
                        "wash",
                        "dry_clean",
                        "maintenance",
                        "mark_dirty",
                        "clear_dirty",
                    ]
                    .iter()
                    .any(|s| doc["action"] == *s),
                )?;
            }
        }
        "care_config" => {
            fields(
                doc,
                &[
                    "schema_version",
                    "id",
                    "member_id",
                    "clothing_id",
                    "wash_after_wears",
                    "revision",
                    "created_at",
                    "updated_at",
                ],
            )?;
            id(doc, "id")?;
            id(doc, "clothing_id")?;
            check(doc["clothing_id"] == entity)?;
            revision(doc, "revision")?;
            check(
                doc["wash_after_wears"].is_null()
                    || doc["wash_after_wears"]
                        .as_u64()
                        .is_some_and(|v| v > 0 && v <= i64::MAX as u64),
            )?;
            timestamp(doc, "created_at")?;
            timestamp(doc, "updated_at")?;
        }
        "insights_preferences" => {
            fields(
                doc,
                &[
                    "schema_version",
                    "id",
                    "member_id",
                    "stale_days",
                    "revision",
                    "created_at",
                    "updated_at",
                ],
            )?;
            id(doc, "id")?;
            check(member == entity)?;
            revision(doc, "revision")?;
            check(
                doc["stale_days"]
                    .as_u64()
                    .is_some_and(|v| (1..=36500).contains(&v)),
            )?;
            timestamp(doc, "created_at")?;
            timestamp(doc, "updated_at")?;
        }
        _ => return Err(error("sync_invalid")),
    }
    Ok(())
}
