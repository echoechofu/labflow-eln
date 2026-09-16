//! Shared validation for the existing, bounded Protocol schema and Record values.
use serde_json::Value;
use std::collections::HashSet;

const SUMMARY_KEYS: &[&str] = &[
    "date",
    "input_sample_summary",
    "output_sample_summary",
    "plate_layout_summary",
    "treatment_summary",
    "condition_groups_summary",
];

fn nonempty<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| format!("Protocol {key} must be a nonempty string"))
}

fn valid_key(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= 64
        && key
            .bytes()
            .enumerate()
            .all(|(i, c)| c.is_ascii_alphabetic() || c == b'_' || (i > 0 && c.is_ascii_digit()))
}

fn validate_scalar(field: &Value, text: &str) -> Result<(), String> {
    let key = nonempty(field, "key")?;
    match field.get("kind").and_then(Value::as_str) {
        Some("number") => {
            let number = text
                .trim()
                .parse::<f64>()
                .ok()
                .filter(|n| n.is_finite())
                .ok_or_else(|| format!("{key} must be a finite number"))?;
            if field
                .get("min")
                .and_then(Value::as_f64)
                .is_some_and(|min| number < min)
                || field
                    .get("max")
                    .and_then(Value::as_f64)
                    .is_some_and(|max| number > max)
            {
                return Err(format!("{key} is outside its allowed range"));
            }
        }
        Some("select") => {
            // Legacy schemas may omit options and rely on a specialized
            // executor (e.g. plate capacity). New schemas require options.
            if field
                .get("options")
                .and_then(Value::as_array)
                .is_some_and(|options| !options.iter().any(|o| o.as_str() == Some(text)))
            {
                return Err(format!("{key} is not an allowed option"));
            }
        }
        _ => {}
    }
    Ok(())
}

fn validate_template(template: &str, keys: &HashSet<&str>) -> Result<(), String> {
    if template.trim().is_empty() {
        return Err("Record template cannot be empty".into());
    }
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        let after = &rest[start + 2..];
        let end = after
            .find("}}")
            .ok_or("Record template contains an unclosed placeholder")?;
        let key = &after[..end];
        // Rendering uses exact keys. Reject whitespace instead of accepting an unrenderable token.
        if !keys.contains(key) && !SUMMARY_KEYS.contains(&key) {
            return Err(format!("Unknown Record template placeholder: {key}"));
        }
        rest = &after[end + 2..];
    }
    Ok(())
}

pub fn validate_schema(spec: &Value) -> Result<(), String> {
    let fields = spec
        .get("fields")
        .and_then(Value::as_array)
        .ok_or("Protocol fields must be an array")?;
    if fields.len() > 100 {
        return Err("A Protocol supports at most 100 fields".into());
    }
    let mut keys = HashSet::new();
    for field in fields {
        let key = nonempty(field, "key")?;
        if !valid_key(key) || SUMMARY_KEYS.contains(&key) || !keys.insert(key) {
            return Err(format!("Invalid or duplicate Protocol field key: {key}"));
        }
        nonempty(field, "label")?;
        let kind = nonempty(field, "kind")?;
        if ![
            "text",
            "number",
            "select",
            "samples",
            "plate_layout",
            "condition_groups",
        ]
        .contains(&kind)
        {
            return Err(format!("Unsupported Protocol field kind: {kind}"));
        }
        if field.get("required").is_some_and(|v| !v.is_boolean()) {
            return Err(format!("{key}.required must be boolean"));
        }
        if field.get("unit").is_some_and(|v| !v.is_string()) {
            return Err(format!("{key}.unit must be a string"));
        }
        for bound in ["min", "max"] {
            if let Some(value) = field.get(bound) {
                if kind != "number" || value.as_f64().is_none_or(|n| !n.is_finite()) {
                    return Err(format!("{key}.{bound} must be a finite numeric bound"));
                }
            }
        }
        if let (Some(min), Some(max)) = (
            field.get("min").and_then(Value::as_f64),
            field.get("max").and_then(Value::as_f64),
        ) {
            if min > max {
                return Err(format!("{key}.min must not exceed max"));
            }
        }
        if kind == "select" {
            let options = field
                .get("options")
                .and_then(Value::as_array)
                .ok_or_else(|| format!("{key} requires options"))?;
            let mut unique = HashSet::new();
            if options.is_empty()
                || options.iter().any(|o| {
                    o.as_str()
                        .is_none_or(|s| s.trim().is_empty() || !unique.insert(s))
                })
            {
                return Err(format!("{key} requires nonempty unique options"));
            }
        }
        if let Some(default) = field.get("defaultValue") {
            let text = default
                .as_str()
                .ok_or_else(|| format!("{key}.defaultValue must be a string"))?;
            if !text.trim().is_empty() {
                validate_scalar(field, text)?;
            }
        }
        if let Some(types) = field.get("visibleForInputTypes") {
            let types = types
                .as_array()
                .ok_or("visibleForInputTypes must be an array")?;
            if types.is_empty()
                || types.iter().any(|t| {
                    t.as_str()
                        .is_none_or(|s| s.is_empty() || s != s.to_uppercase())
                })
            {
                return Err("visibleForInputTypes requires canonical Sample types".into());
            }
        }
    }
    for field in fields {
        if let Some(condition) = field.get("visibleWhen") {
            let dependency = nonempty(condition, "key")?;
            let expected = condition
                .get("value")
                .and_then(Value::as_str)
                .ok_or("visibleWhen.value must be a string")?;
            if !keys.contains(dependency) || field["key"].as_str() == Some(dependency) {
                return Err("visibleWhen must reference another existing field".into());
            }
            let referenced = fields
                .iter()
                .find(|f| f["key"].as_str() == Some(dependency))
                .unwrap();
            validate_scalar(referenced, expected)?;
        }
    }
    if let Some(selector) = spec.get("templateSelector") {
        let selector = selector
            .as_str()
            .ok_or("templateSelector must be a field key")?;
        let field = fields
            .iter()
            .find(|f| f["key"].as_str() == Some(selector) && f["kind"] == "select")
            .ok_or("templateSelector must reference a select field")?;
        let variants = spec
            .get("templateVariants")
            .and_then(Value::as_object)
            .ok_or("Template variants are required")?;
        for option in field["options"].as_array().unwrap() {
            let template = variants
                .get(option.as_str().unwrap())
                .and_then(Value::as_str)
                .ok_or("Missing template variant")?;
            validate_template(template, &keys)?;
        }
        for template in variants.values() {
            validate_template(
                template.as_str().ok_or("Template variant must be text")?,
                &keys,
            )?;
        }
    } else {
        validate_template(nonempty(spec, "template")?, &keys)?;
    }
    // Execution is composed by the service or copied from a persisted source, never client code.
    let execution = spec
        .get("execution")
        .ok_or("Protocol execution is required")?;
    nonempty(execution, "eventType")?;
    let mode = nonempty(execution, "outputMode")?;
    if ![
        "one",
        "count",
        "per_input",
        "per_input_count",
        "per_input_conditions",
        "per_input_types",
        "same_sample",
        "plate_or_dish",
        "plate_wells",
        "none",
    ]
    .contains(&mode)
    {
        return Err("Unsupported Protocol output mode".into());
    }
    if mode == "per_input_types" {
        let rules = execution
            .get("outputRules")
            .and_then(Value::as_array)
            .ok_or("Multi-type output requires outputRules")?;
        if !(2..=16).contains(&rules.len()) {
            return Err("Multi-type output requires 2–16 output rules".into());
        }
        let mut output_types = HashSet::new();
        let mut total = 0_u64;
        for rule in rules {
            let sample_type = nonempty(rule, "sampleType")?;
            if sample_type.len() > 32
                || !sample_type.chars().enumerate().all(|(index, character)| {
                    character.is_ascii_uppercase()
                        || character.is_ascii_digit() && index > 0
                        || character == '_' && index > 0
                })
                || !output_types.insert(sample_type)
            {
                return Err(format!(
                    "Invalid or duplicate multi-type output: {sample_type}"
                ));
            }
            let count = rule
                .get("count")
                .and_then(Value::as_u64)
                .filter(|count| (1..=96).contains(count))
                .ok_or("Each multi-type output count must be 1–96")?;
            total += count;
        }
        if total > 96 {
            return Err("Multi-type outputs may total at most 96 per input".into());
        }
    }
    if mode == "same_sample" && execution["consumptionPolicy"] == "consume" {
        return Err("A consumed Sample cannot continue as the output".into());
    }
    if spec.get("terminalAssay").is_some() && !keys.contains("assay_items") {
        return Err("Terminal Assay requires assay_items".into());
    }
    Ok(())
}

pub fn validate_values(spec: &Value, values: &Value, input_types: &[String]) -> Result<(), String> {
    let values = values
        .as_object()
        .ok_or("Record values must be an object")?;
    for field in spec
        .get("fields")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let visible_by_value = field.get("visibleWhen").is_none_or(|condition| {
            condition["key"].as_str().and_then(|key| values.get(key)) == condition.get("value")
        });
        let visible_by_type = field
            .get("visibleForInputTypes")
            .and_then(Value::as_array)
            .is_none_or(|types| {
                types.iter().any(|t| {
                    t.as_str()
                        .is_some_and(|t| input_types.iter().any(|i| i == t))
                })
            });
        if !visible_by_value || !visible_by_type {
            continue;
        }
        let key = nonempty(field, "key")?;
        let text = match values.get(key) {
            None => "",
            Some(value) => value
                .as_str()
                .ok_or_else(|| format!("{key} must be text"))?,
        };
        if text.trim().is_empty() {
            if field["required"] == true {
                return Err(format!("Missing required Protocol field: {key}"));
            }
        } else {
            validate_scalar(field, text)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn all_builtin_schemas_validate() {
        for builtin in crate::protocol_catalog::builtins() {
            validate_schema(&serde_json::from_str(builtin.schema).unwrap())
                .unwrap_or_else(|e| panic!("{}: {e}", builtin.id));
        }
    }

    #[test]
    fn visible_fields_validate_numbers_options_and_required_values() {
        let spec = json!({"fields":[
            {"key":"mode","kind":"select","options":["yes","no"],"label":"Mode"},
            {"key":"dose","kind":"number","label":"Dose","required":true,"min":0,"max":10,"visibleWhen":{"key":"mode","value":"yes"}},
            {"key":"plate","kind":"text","label":"Plate","required":true,"visibleForInputTypes":["PLATE"]}
        ],"template":"{{dose}}","execution":{"eventType":"custom:test","outputMode":"none"}});
        validate_schema(&spec).unwrap();
        validate_values(&spec, &json!({"mode":"no"}), &["CELL".into()]).unwrap();
        validate_values(&spec, &json!({"mode":"yes","dose":"2.5"}), &["CELL".into()]).unwrap();
        for values in [
            json!({"mode":"invalid"}),
            json!({"mode":"yes"}),
            json!({"mode":"yes","dose":"NaN"}),
            json!({"mode":"yes","dose":"11"}),
        ] {
            assert!(validate_values(&spec, &values, &["CELL".into()]).is_err());
        }
        assert!(validate_values(&spec, &json!({"mode":"no"}), &["PLATE".into()]).is_err());
    }

    #[test]
    fn invalid_field_schema_is_rejected() {
        let base = json!({"fields":[{"key":"dose","kind":"number","label":"Dose"}],"template":"{{dose}}","execution":{"eventType":"custom:test","outputMode":"none"}});
        for field in [
            json!({"key":"dose","kind":"number","label":"Dose","min":2,"max":1}),
            json!({"key":"dose","kind":"number","label":"Dose","defaultValue":"NaN"}),
            json!({"key":"dose","kind":"select","label":"Dose","options":["a","a"]}),
        ] {
            let mut spec = base.clone();
            spec["fields"] = json!([field]);
            assert!(validate_schema(&spec).is_err());
        }
        let mut duplicate = base.clone();
        duplicate["fields"] = json!([base["fields"][0], base["fields"][0]]);
        assert!(validate_schema(&duplicate).is_err());
        let mut unknown = base;
        unknown["template"] = json!("{{missing}}");
        assert!(validate_schema(&unknown).is_err());
    }
}
