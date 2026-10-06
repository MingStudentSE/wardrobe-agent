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
fn controls(value: &Value, member: &str) -> Result<(), ApiError> {
    fields(
        value,
        &[
            "member_id",
            "strategy",
            "provider_id",
            "json_output",
            "output_format",
            "operation_key",
            "timezone",
            "limit",
            "fixed_ids",
            "excluded_ids",
            "inventory_priority_ids",
            "avoid_recent",
            "target",
            "occasion",
            "weather",
            "excluded_sets",
            "source_intent",
            "retained_ids",
            "action_excluded_ids",
            "replacement_category",
            "retained_items",
        ],
    )?;
    check(value["member_id"] == member)?;
    text(value, "operation_key", 1, 200)?;
    check(string(value, "timezone")?.parse::<chrono_tz::Tz>().is_ok())?;
    for key in ["provider_id", "source_intent"] {
        if value.get(key).is_some() {
            id(value, key)?;
        }
    }
    for key in [
        "fixed_ids",
        "excluded_ids",
        "inventory_priority_ids",
        "retained_ids",
        "action_excluded_ids",
    ] {
        if let Some(v) = value.get(key) {
            ids(v)?;
        }
    }
    if let Some(v) = value.get("excluded_sets") {
        for set in v
            .as_array()
            .filter(|v| v.len() <= 100)
            .ok_or_else(|| error("sync_invalid"))?
        {
            ids(set)?;
        }
    }
    for key in ["json_output", "avoid_recent"] {
        if let Some(v) = value.get(key) {
            check(v.is_boolean())?;
        }
    }
    if let Some(v) = value.get("limit") {
        check(v.as_u64().is_some_and(|n| (1..=5).contains(&n)))?;
    }
    for (key, values) in [
        ("strategy", &["rules", "ai"][..]),
        (
            "output_format",
            &["plain", "json_object", "llama_cpp_schema"][..],
        ),
        ("target", &["balanced", "least_worn", "favorites"][..]),
        ("occasion", &["daily", "work", "formal", "sport"][..]),
    ] {
        if let Some(v) = value.get(key) {
            check(values.iter().any(|s| v == *s))?;
        }
    }
    if let Some(v) = value.get("replacement_category") {
        check(crate::CATEGORIES.iter().any(|s| v == *s))?;
    }
    if let Some(v) = value.get("retained_items") {
        for item in v
            .as_array()
            .filter(|v| v.len() <= 100)
            .ok_or_else(|| error("sync_invalid"))?
        {
            fields(item, &["clothing_id", "role", "layer"])?;
            id(item, "clothing_id")?;
            text(item, "role", 1, 40)?;
            check(
                item["layer"]
                    .as_i64()
                    .is_some_and(|n| (-100..=100).contains(&n)),
            )?;
        }
    }
    if let Some(v) = value.get("weather") {
        fields(v, &["mode", "temperature_c", "location", "obtained_at"])?;
        check(
            ["off", "manual", "automatic"]
                .iter()
                .any(|s| v["mode"] == *s),
        )?;
        if let Some(n) = v.get("temperature_c") {
            check(
                n.as_f64()
                    .is_some_and(|n| n.is_finite() && (-80.0..=60.0).contains(&n)),
            )?;
        }
        if v.get("location").is_some() {
            text(v, "location", 1, 120)?;
        }
        if v.get("obtained_at").is_some() {
            timestamp(v, "obtained_at")?;
        }
    }
    Ok(())
}
pub(super) fn event(entity: &str, doc: &Value) -> Result<(), ApiError> {
    fields(
        doc,
        &[
            "schema_version",
            "id",
            "member_id",
            "root_id",
            "revision",
            "revoked",
            "intent_id",
            "proposal_index",
            "proposal",
            "inventory_snapshots",
            "inventory_observed_at",
            "controls",
            "reaction",
            "wear_event_id",
            "day_scope",
            "clothing_ids",
            "scores",
            "created_at",
            "supersedes_event_id",
            "reason",
        ],
    )?;
    id(doc, "id")?;
    id(doc, "root_id")?;
    id(doc, "intent_id")?;
    revision(doc, "revision")?;
    check(
        doc["id"] == entity
            && doc["revoked"].is_boolean()
            && doc["proposal_index"].as_u64().is_some_and(|n| n < 5),
    )?;
    if doc["revision"] == 1 {
        check(doc["root_id"] == doc["id"] && doc["supersedes_event_id"].is_null())?;
    } else {
        id(doc, "supersedes_event_id")?;
        check(doc["id"] != doc["root_id"] && doc["id"] != doc["supersedes_event_id"])?;
    }
    if doc["revoked"] == true {
        text(doc, "reason", 1, 2000)?;
    }
    timestamp(doc, "created_at")?;
    timestamp(doc, "inventory_observed_at")?;
    let member = string(doc, "member_id")?;
    controls(&doc["controls"], member)?;
    check(
        [
            "like",
            "dislike",
            "skip",
            "day_exclude",
            "actual_wore",
            "changed",
        ]
        .iter()
        .any(|s| doc["reaction"] == *s),
    )?;
    if !doc["wear_event_id"].is_null() {
        id(doc, "wear_event_id")?;
    }
    scores(&doc["scores"])?;
    ids(&doc["clothing_ids"])?;
    let proposal = &doc["proposal"];
    fields(
        proposal,
        &[
            "items",
            "complete",
            "missing_roles",
            "repeat_relaxed",
            "reasons",
            "explanation",
            "occasion_verified",
        ],
    )?;
    let proposal_items = proposal["items"]
        .as_array()
        .filter(|v| !v.is_empty() && v.len() <= 100)
        .ok_or_else(|| error("sync_invalid"))?;
    let mut proposal_ids = std::collections::BTreeSet::new();
    for item in proposal_items {
        fields(item, &["clothing_id", "name", "category", "role", "layer"])?;
        id(item, "clothing_id")?;
        check(proposal_ids.insert(string(item, "clothing_id")?))?;
        text(item, "name", 1, 120)?;
        check(
            crate::CATEGORIES
                .iter()
                .any(|category| item["category"] == *category),
        )?;
        text(item, "role", 1, 40)?;
        check(
            item["layer"]
                .as_i64()
                .is_some_and(|n| (-100..=100).contains(&n)),
        )?;
    }
    check(proposal["complete"].is_boolean() && proposal["repeat_relaxed"].is_boolean())?;
    if let Some(value) = proposal.get("occasion_verified") {
        check(value.is_boolean())?;
    }
    for key in ["missing_roles", "reasons"] {
        let values = proposal[key]
            .as_array()
            .filter(|v| v.len() <= 100)
            .ok_or_else(|| error("sync_invalid"))?;
        for v in values {
            check(v.as_str().is_some_and(|s| s.chars().count() <= 2000))?;
        }
    }
    if proposal.get("explanation").is_some() {
        text(proposal, "explanation", 0, 2000)?;
    }
    let snapshots = doc["inventory_snapshots"]
        .as_array()
        .filter(|v| !v.is_empty() && v.len() <= 100)
        .ok_or_else(|| error("sync_invalid"))?;
    let mut seen = std::collections::BTreeSet::new();
    for snapshot in snapshots {
        clothing(snapshot, member)?;
        check(seen.insert(string(snapshot, "id")?))?;
    }
    let selected: std::collections::BTreeSet<_> = proposal["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["clothing_id"].as_str().unwrap())
        .collect();
    check(seen == selected)?;
    if !doc["day_scope"].is_null() {
        let day = &doc["day_scope"];
        fields(day, &["timezone", "local_date", "expires_at"])?;
        let zone = string(day, "timezone")?
            .parse::<chrono_tz::Tz>()
            .map_err(|_| error("sync_invalid"))?;
        check(chrono::NaiveDate::parse_from_str(string(day, "local_date")?, "%Y-%m-%d").is_ok())?;
        timestamp(day, "expires_at")?;
        let _ = zone;
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
