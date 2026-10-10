use super::*;
fn scores(value: &Value) -> Result<(), ApiError> {
    fields(value, &["overall", "comfort", "style"])?;
    for key in ["overall", "comfort", "style"] {
        check(value[key].is_null() || value[key].as_u64().is_some_and(|n| (1..=5).contains(&n)))?;
    }
    Ok(())
}
fn ids(value: &Value) -> Result<(), ApiError> {
    let values = value
        .as_array()
        .filter(|v| v.len() <= 100)
        .ok_or_else(|| error("sync_invalid"))?;
    let mut seen = std::collections::BTreeSet::new();
    for value in values {
        let id = value.as_str().ok_or_else(|| error("sync_invalid"))?;
        check(uuid::Uuid::parse_str(id).is_ok() && seen.insert(id))?;
    }
    Ok(())
}
pub(super) fn event(entity: &str, doc: &Value) -> Result<(), ApiError> {
    crate::feedback::validate_snapshot(doc).map_err(|_| error("sync_invalid"))?;
    check(doc["id"] == entity)?;
    let member = string(doc, "member_id")?;
    for snapshot in doc["inventory_snapshots"]
        .as_array()
        .ok_or_else(|| error("sync_invalid"))?
    {
        // Transport retains its stricter closed clothing envelope validation.
        clothing(snapshot, member)?;
    }
    Ok(())
}
pub(super) fn receipt(entity: &str, doc: &Value) -> Result<(), ApiError> {
    fields(
        doc,
        &[
            "schema_version",
            "member_id",
            "operation_key",
            "request",
            "result",
        ],
    )?;
    let member = string(doc, "member_id")?;
    text(doc, "operation_key", 1, 200)?;
    check(entity == format!("{member}:{}", string(doc, "operation_key")?))?;
    let result: Value =
        serde_json::from_str(string(doc, "result")?).map_err(|_| error("sync_invalid"))?;
    validate("feedback_events", string(&result, "id")?, &result)?;
    check(result["member_id"] == member)?;
    let request: Value =
        serde_json::from_str(string(doc, "request")?).map_err(|_| error("sync_invalid"))?;
    let (operation, input) = if request.get("operation").is_some() {
        fields(&request, &["operation", "input"])?;
        (string(&request, "operation")?, &request["input"])
    } else {
        ("feedback.record", &request)
    };
    let allowed = match operation {
        "feedback.record" => &[
            "member_id",
            "operation_key",
            "intent_id",
            "proposal_index",
            "reaction",
            "scores",
            "wear",
            "timezone",
            "clothing_ids",
        ][..],
        "feedback.correct" => &[
            "member_id",
            "operation_key",
            "root_id",
            "expected_revision",
            "reaction",
            "scores",
        ][..],
        "feedback.revoke" => &[
            "member_id",
            "operation_key",
            "root_id",
            "expected_revision",
            "reason",
        ][..],
        _ => return Err(error("sync_invalid")),
    };
    fields(input, allowed)?;
    check(input["member_id"] == member && input["operation_key"] == doc["operation_key"])?;
    if operation == "feedback.record" {
        check(
            input["intent_id"] == result["intent_id"]
                && input["proposal_index"] == result["proposal_index"]
                && input["reaction"] == result["reaction"]
                && result["revision"] == 1,
        )?;
    } else {
        check(
            input["root_id"] == result["root_id"]
                && input["expected_revision"]
                    .as_u64()
                    .and_then(|n| n.checked_add(1))
                    == result["revision"].as_u64(),
        )?;
    }
    if let Some(v) = input.get("scores") {
        scores(v)?;
    }
    if operation == "feedback.revoke" {
        check(result["revoked"] == true && input["reason"] == result["reason"])?;
    }
    if let Some(v) = input.get("clothing_ids") {
        ids(v)?;
    }
    if let Some(wear) = input.get("wear") {
        fields(
            wear,
            &[
                "local_date",
                "timezone",
                "local_time",
                "clothing_ids",
                "notes",
            ],
        )?;
        if let Some(v) = wear.get("clothing_ids") {
            ids(v)?;
        }
        check(
            chrono::NaiveDate::parse_from_str(string(wear, "local_date")?, "%Y-%m-%d").is_ok()
                && string(wear, "timezone")?.parse::<chrono_tz::Tz>().is_ok(),
        )?;
        if wear.get("notes").is_some() {
            text(wear, "notes", 0, 2000)?;
        }
        if let Some(time) = wear.get("local_time") {
            check(
                time.is_null()
                    || time
                        .as_str()
                        .is_some_and(|s| chrono::NaiveTime::parse_from_str(s, "%H:%M").is_ok()),
            )?;
        }
    }
    Ok(())
}
