use rusqlite::{params, Connection, Transaction};
use serde_json::{json, Map, Value};

#[allow(dead_code)]
#[derive(Debug)]
pub struct ExecutionResult {
    pub task: Value,
    pub input_ids: Vec<String>,
    pub output_ids: Vec<String>,
    pub rendered_content: String,
}

fn string_value<'a>(values: &'a Value, key: &str) -> Option<&'a str> {
    values
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn canonical_sample_type(value: &str) -> String {
    value.to_uppercase()
}

fn sample_code_suffix(canonical_type: &str) -> &str {
    if canonical_type == "CDNA" {
        "cDNA"
    } else {
        canonical_type
    }
}

fn next_sample_code(
    tx: &Transaction<'_>,
    experiment_id: &str,
    prefix: &str,
    sample_type: &str,
) -> Result<String, String> {
    let canonical_type = canonical_sample_type(sample_type);
    let count: i64 = tx
        .query_row(
            "SELECT count(*) FROM samples WHERE experiment_id=?1 AND upper(sample_type)=?2",
            params![experiment_id, canonical_type],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    for sequence in (count + 1)..=(count + 10_000) {
        let code = format!(
            "{prefix}-{}{sequence:02}",
            sample_code_suffix(&canonical_type)
        );
        let exists: i64 = tx
            .query_row(
                "SELECT count(*) FROM samples WHERE workspace_id='local' AND sample_code=?1",
                [&code],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        if exists == 0 {
            return Ok(code);
        }
    }
    Err("Unable to allocate Sample code".into())
}

fn resolve_experiment_inputs(
    tx: &Transaction<'_>,
    experiment_id: &str,
    execution: &Value,
    supplied: &[String],
) -> Result<Vec<(String, String)>, String> {
    if supplied.is_empty() {
        return Err("Select or register at least one Sample in this Experiment".into());
    }
    if execution
        .get("inputCardinality")
        .and_then(Value::as_str)
        .unwrap_or("one")
        == "one"
        && supplied.len() != 1
    {
        return Err("This Protocol accepts exactly one input Sample".into());
    }
    let accepted = execution
        .get("inputTypes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_uppercase)
        .collect::<Vec<_>>();
    let mut resolved = Vec::with_capacity(supplied.len());
    for id in supplied {
        if resolved.iter().any(|(existing, _)| existing == id) {
            return Err("The same input Sample was selected more than once".into());
        }
        let consumed: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sample_usages WHERE sample_id=?1 AND usage_type='consumed')",
                [id],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        if consumed {
            return Err(format!("Input Sample {id} has already been consumed"));
        }
        let sample_type: String = tx
            .query_row(
                "SELECT upper(sample.sample_type)
                 FROM samples sample
                 WHERE sample.id=?1 AND sample.experiment_id=?2 AND sample.archived_at IS NULL
                   AND NOT EXISTS (SELECT 1 FROM sample_usages usage WHERE usage.sample_id=sample.id AND usage.usage_type='consumed')",
                params![id, experiment_id],
                |row| row.get(0),
            )
            .map_err(|_| {
                "Input Sample must be available in this Experiment and must not be consumed"
                    .to_string()
            })?;
        if !accepted.is_empty() && !accepted.contains(&sample_type) {
            return Err(format!("Protocol does not accept {sample_type} inputs"));
        }
        resolved.push((id.clone(), sample_type));
    }
    Ok(resolved)
}

fn insert_sample(
    tx: &Transaction<'_>,
    id: &str,
    experiment_id: &str,
    code: &str,
    sample_type: &str,
    record_id: &str,
    parent_id: Option<&str>,
    display_name: &str,
    occurred_at: &str,
    metadata: &Value,
) -> Result<(), String> {
    let canonical_type = canonical_sample_type(sample_type);
    tx.execute(
        "INSERT INTO samples (id,workspace_id,experiment_id,sample_code,sample_type,source_record_id,parent_sample_id,display_name,created_at,lineage_status,metadata_json) VALUES (?1,'local',?2,?3,?4,?5,?6,?7,?8,'complete',?9)",
        params![id, experiment_id, code, canonical_type, record_id, parent_id, display_name, occurred_at, metadata.to_string()],
    ).map_err(|error| error.to_string())?;
    if let Some(parent) = parent_id {
        tx.execute("INSERT INTO sample_relations (id,parent_sample_id,child_sample_id,relation_type) VALUES (?1,?2,?3,'derived_from')", params![format!("relation-{id}"), parent, id]).map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn insert_external_sample(
    tx: &Transaction<'_>,
    id: &str,
    experiment_id: &str,
    code: &str,
    sample_type: &str,
    display_name: &str,
    occurred_at: &str,
    metadata: &Value,
) -> Result<(), String> {
    let canonical_type = canonical_sample_type(sample_type);
    let registered: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sample_types WHERE canonical_type=?1 AND archived_at IS NULL)",
            [&canonical_type],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if !registered {
        return Err(format!("Sample type {canonical_type} is not registered"));
    }
    tx.execute(
        "INSERT INTO samples (id,workspace_id,experiment_id,sample_code,sample_type,source_record_id,parent_sample_id,display_name,created_at,lineage_status,metadata_json,origin) VALUES (?1,'local',?2,?3,?4,NULL,NULL,?5,?6,'complete',?7,'external')",
        params![id, experiment_id, code, canonical_type, display_name, occurred_at, metadata.to_string()],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

fn create_external_inputs(
    tx: &Transaction<'_>,
    record_id: &str,
    experiment_id: &str,
    experiment_code: &str,
    task_date: &str,
    drafts: &[Value],
) -> Result<Vec<String>, String> {
    if drafts.len() > 96 {
        return Err("At most 96 existing Samples can be registered at once".into());
    }
    drafts
        .iter()
        .enumerate()
        .map(|(index, draft)| {
            let sample_type =
                string_value(draft, "sampleType").ok_or("External Sample type is required")?;
            let display_name =
                string_value(draft, "displayName").ok_or("External Sample label is required")?;
            let code = next_sample_code(tx, experiment_id, experiment_code, sample_type)?;
            let id = format!("sample-{record_id}-external-{index}");
            let mut metadata = draft
                .get("metadata")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            metadata.insert("registered_during_record_id".into(), json!(record_id));
            insert_external_sample(
                tx,
                &id,
                experiment_id,
                &code,
                sample_type,
                display_name,
                task_date,
                &Value::Object(metadata),
            )?;
            Ok(id)
        })
        .collect()
}

fn validate_required_fields(spec: &Value, values: &Value) -> Result<(), String> {
    for field in spec
        .get("fields")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if field
            .get("required")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            let key = field
                .get("key")
                .and_then(Value::as_str)
                .ok_or("Protocol field is missing key")?;
            if string_value(values, key).is_none() {
                return Err(format!("Missing required Protocol field: {key}"));
            }
        }
    }
    Ok(())
}

fn render_template(spec: &Value, values: &Value, date: &str) -> String {
    let selected_template = spec
        .get("templateSelector")
        .and_then(Value::as_str)
        .and_then(|selector| string_value(values, selector))
        .and_then(|selection| spec.get("templateVariants")?.get(selection))
        .and_then(Value::as_str)
        .or_else(|| spec.get("template").and_then(Value::as_str))
        .unwrap_or("");
    let mut rendered = selected_template.replace("{{date}}", date);
    if let Some(fields) = spec.get("fields").and_then(Value::as_array) {
        for field in fields {
            if let Some(key) = field.get("key").and_then(Value::as_str) {
                rendered = rendered.replace(
                    &format!("{{{{{key}}}}}"),
                    string_value(values, key).unwrap_or(""),
                );
            }
        }
    }
    rendered
}

fn resolve_or_create_input(
    tx: &Transaction<'_>,
    record_id: &str,
    experiment_id: &str,
    experiment_code: &str,
    task_date: &str,
    event_type: &str,
    values: &Value,
    supplied: &[String],
) -> Result<Option<(String, String)>, String> {
    if supplied.len() > 1 {
        return Err("This Protocol accepts one input Sample".into());
    }
    if let Some(id) = supplied.first() {
        let sample_type: String = tx.query_row("SELECT upper(sample_type) FROM samples WHERE id=?1 AND experiment_id=?2 AND archived_at IS NULL AND NOT EXISTS (SELECT 1 FROM sample_usages usage WHERE usage.sample_id=samples.id AND usage.usage_type='consumed')", params![id, experiment_id], |row| row.get(0)).map_err(|_| "Input Sample must exist in this Experiment and must not be consumed".to_string())?;
        return Ok(Some((id.clone(), sample_type)));
    }
    if event_type == "thaw" {
        return Ok(None);
    }
    let (sample_type, display_name) = match event_type {
        "passage" | "plating" => (
            "CELL",
            string_value(values, "cell_name")
                .ok_or("Select an existing CELL or enter a new cell name")?,
        ),
        "treatment" => {
            let kind = string_value(values, "new_object_type")
                .ok_or("Select an existing Sample or choose a new object type")?;
            let normalized = match kind {
                "孔板" => "PLATE",
                "培养皿" => "DISH",
                "孔" => "WELL",
                _ => return Err("Invalid new treatment object type".into()),
            };
            (
                normalized,
                string_value(values, "new_object_name").unwrap_or(kind),
            )
        }
        _ => return Ok(None),
    };
    let id = format!("sample-{record_id}-source");
    let code = next_sample_code(tx, experiment_id, experiment_code, sample_type)?;
    let mut metadata = values.as_object().cloned().unwrap_or_default();
    metadata.insert("registered_during_record_id".into(), json!(record_id));
    insert_external_sample(
        tx,
        &id,
        experiment_id,
        &code,
        sample_type,
        display_name,
        task_date,
        &Value::Object(metadata),
    )?;
    Ok(Some((id, sample_type.into())))
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn execute(
    connection: &mut Connection,
    task_id: &str,
    protocol_id: &str,
    record_id: &str,
    values: Value,
    supplied_inputs: Vec<String>,
) -> Result<ExecutionResult, String> {
    execute_with_external(
        connection,
        task_id,
        protocol_id,
        record_id,
        values,
        supplied_inputs,
        Vec::new(),
    )
}

pub fn execute_with_external(
    connection: &mut Connection,
    task_id: &str,
    protocol_id: &str,
    record_id: &str,
    values: Value,
    supplied_inputs: Vec<String>,
    external_inputs: Vec<Value>,
) -> Result<ExecutionResult, String> {
    let tx = connection
        .transaction()
        .map_err(|error| error.to_string())?;
    let (experiment_id, experiment_code, title, task_start): (String, String, String, String) = tx.query_row(
        "SELECT t.experiment_id,e.experiment_code,t.title,t.start_time FROM tasks t JOIN experiments e ON e.id=t.experiment_id WHERE t.id=?1 AND t.record_id IS NULL",
        [task_id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    ).map_err(|_| "Task is missing or already has a Record".to_string())?;
    let (protocol_name, version, schema): (String, i64, String) = tx.query_row(
        "SELECT p.name,p.active_version,pv.schema_json FROM protocols p JOIN protocol_versions pv ON pv.protocol_id=p.id AND pv.version_number=p.active_version WHERE p.id=?1",
        [protocol_id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    ).map_err(|_| "Protocol not found".to_string())?;
    let spec: Value =
        serde_json::from_str(&schema).map_err(|_| "Protocol schema is invalid".to_string())?;
    validate_required_fields(&spec, &values)?;
    let execution = spec
        .get("execution")
        .ok_or("Protocol has no execution rule")?;
    let event_type = execution
        .get("eventType")
        .and_then(Value::as_str)
        .ok_or("Protocol execution is missing eventType")?;
    let task_date = task_start.split('T').next().unwrap_or(&task_start);
    let mut rendered = render_template(&spec, &values, task_date);
    if rendered.trim().is_empty() {
        return Err("Protocol template rendered empty content".into());
    }
    let snapshot = json!({"name":protocol_name,"version":version,"schema":spec});
    tx.execute("INSERT INTO records (id,task_id,experiment_id,protocol_id,protocol_snapshot_json,current_data_json,updated_at) VALUES (?1,?2,?3,?4,?5,'{}',?6)", params![record_id,task_id,experiment_id,protocol_id,snapshot.to_string(),task_date]).map_err(|error| error.to_string())?;
    if spec.get("terminalAssay").is_some() {
        let items = string_value(&values, "assay_items")
            .ok_or("Terminal assay requires at least one assay item")?;
        crate::terminal_assay::create_items(&tx, record_id, items)?;
    }
    let mut selected_inputs = supplied_inputs;
    selected_inputs.extend(create_external_inputs(
        &tx,
        record_id,
        &experiment_id,
        &experiment_code,
        task_date,
        &external_inputs,
    )?);
    let inputs = if matches!(
        execution.get("inputSource").and_then(Value::as_str),
        Some("parent_task_outputs" | "experiment_samples")
    ) {
        resolve_experiment_inputs(&tx, &experiment_id, execution, &selected_inputs)?
    } else {
        resolve_or_create_input(
            &tx,
            record_id,
            &experiment_id,
            &experiment_code,
            task_date,
            event_type,
            &values,
            &selected_inputs,
        )?
        .into_iter()
        .collect()
    };
    let input = inputs.first().cloned();
    let input_ids: Vec<String> = inputs.iter().map(|(id, _)| id.clone()).collect();
    let input_type = input.as_ref().map(|(_, kind)| kind.as_str());
    let input_summary = input_ids
        .iter()
        .map(|id| {
            tx.query_row("SELECT sample_code FROM samples WHERE id=?1", [id], |row| {
                row.get::<_, String>(0)
            })
            .map_err(|error| error.to_string())
        })
        .collect::<Result<Vec<_>, _>>()?
        .join("、");
    rendered = rendered.replace("{{input_sample_summary}}", &input_summary);
    match event_type {
        "thaw" if input.is_some() => {
            return Err("Cell thaw does not accept an existing input Sample".into())
        }
        "passage" | "plating" if input_type != Some("CELL") => {
            return Err(format!("{event_type} requires a CELL input"))
        }
        "treatment" if !matches!(input_type, Some("PLATE" | "DISH" | "WELL")) => {
            return Err("Treatment requires a PLATE, DISH, or WELL input".into())
        }
        _ => {}
    }
    let plate_assignments = if event_type == "treatment" && input_type == Some("PLATE") {
        let input_id = input.as_ref().map(|(id, _)| id).unwrap();
        let metadata_json: String = tx
            .query_row(
                "SELECT metadata_json FROM samples WHERE id=?1",
                [input_id],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        let metadata: Value = serde_json::from_str(&metadata_json).unwrap_or_else(|_| json!({}));
        let capacity = crate::plate_layout::capacity_from_metadata(&metadata).ok_or(
            "Plate capacity is missing; use a plate created with a supported plate format",
        )?;
        let assignments = if let Some(raw) = string_value(&values, "treatment_groups") {
            crate::plate_layout::parse_and_assign(raw, capacity)?
        } else {
            // Preserve execution of historical Protocol snapshots that supplied
            // explicit well positions before grouped plate layouts were added.
            let factor = string_value(&values, "treatment_type")
                .ok_or("Plate treatment requires treatment groups")?;
            let wells = string_value(&values, "target_wells")
                .ok_or("Plate treatment requires treatment groups")?
                .split(',')
                .map(str::trim)
                .filter(|well| !well.is_empty())
                .collect::<Vec<_>>();
            if wells.len() > capacity {
                return Err(format!(
                    "Treatment requests {} wells, exceeding the {capacity}-well plate",
                    wells.len()
                ));
            }
            wells
                .into_iter()
                .map(|position| crate::plate_layout::WellAssignment {
                    position: position.to_owned(),
                    factor: factor.to_owned(),
                    duration: string_value(&values, "treatment_duration")
                        .unwrap_or("")
                        .to_owned(),
                    group_index: 0,
                })
                .collect()
        };
        rendered = rendered.replace(
            "{{plate_layout_summary}}",
            &crate::plate_layout::summary(&assignments),
        );
        Some(assignments)
    } else {
        if event_type == "treatment" && string_value(&values, "treatment_type").is_none() {
            return Err("Dish or well treatment requires a treatment type".into());
        }
        rendered = rendered.replace("{{plate_layout_summary}}", "");
        None
    };
    let event_id = format!("event-{record_id}");
    tx.execute("INSERT INTO process_events (id,experiment_id,record_id,event_type,occurred_at,parameters_json,provenance,created_at) VALUES (?1,?2,?3,?4,?5,?6,'labflow_recorded',?5)", params![event_id,experiment_id,record_id,event_type,task_date,values.to_string()]).map_err(|error| error.to_string())?;
    for id in &input_ids {
        tx.execute(
            "INSERT INTO event_inputs VALUES (?1,?2)",
            params![event_id, id],
        )
        .map_err(|error| error.to_string())?;
        tx.execute(
            "INSERT INTO record_samples VALUES (?1,?2,'input')",
            params![record_id, id],
        )
        .map_err(|error| error.to_string())?;
    }
    if let Some(consumption_policy) = execution.get("consumptionPolicy").and_then(Value::as_str) {
        let usage_type = match consumption_policy {
            "consume" => "consumed",
            "non_destructive" => "non_destructive",
            "aliquot" => "aliquot",
            _ => return Err("Unsupported Protocol consumption policy".into()),
        };
        for id in &input_ids {
            if usage_type == "consumed" {
                let consumed: bool = tx
                    .query_row(
                        "SELECT EXISTS(SELECT 1 FROM sample_usages WHERE sample_id=?1 AND usage_type='consumed')",
                        [id],
                        |row| row.get(0),
                    )
                    .map_err(|error| error.to_string())?;
                if consumed {
                    return Err(format!("Input Sample {id} has already been consumed"));
                }
            }
            tx.execute(
                "INSERT INTO sample_usages (event_id,sample_id,usage_type,created_at) VALUES (?1,?2,?3,?4)",
                params![event_id, id, usage_type, task_date],
            )
            .map_err(|error| error.to_string())?;
        }
    }
    let output_mode = execution
        .get("outputMode")
        .and_then(Value::as_str)
        .unwrap_or("none");
    if output_mode == "same_sample"
        && execution.get("consumptionPolicy").and_then(Value::as_str) == Some("consume")
    {
        return Err("A consumed Sample cannot continue as the Protocol output".into());
    }
    let (output_type, output_labels, output_parents): (&str, Vec<String>, Vec<Option<String>>) =
        match output_mode {
            "one" => (
                execution
                    .get("outputType")
                    .and_then(Value::as_str)
                    .ok_or("outputType is required")?,
                vec![string_value(&values, "cell_name").unwrap_or("Cell").into()],
                vec![input.as_ref().map(|(id, _)| id.clone())],
            ),
            "count" => {
                let count = string_value(&values, "output_count")
                    .and_then(|v| v.parse::<usize>().ok())
                    .filter(|v| *v > 0 && *v <= 96)
                    .ok_or("Output count must be 1–96")?;
                let labels = (1..=count)
                    .map(|index| format!("Output {index}"))
                    .collect::<Vec<_>>();
                (
                    execution
                        .get("outputType")
                        .and_then(Value::as_str)
                        .unwrap_or("CELL"),
                    labels.clone(),
                    vec![input.as_ref().map(|(id, _)| id.clone()); labels.len()],
                )
            }
            "per_input" => {
                let output_type = execution
                    .get("outputType")
                    .and_then(Value::as_str)
                    .ok_or("outputType is required")?;
                let labels = input_ids
                    .iter()
                    .map(|id| {
                        tx.query_row(
                            "SELECT coalesce(display_name,sample_code) FROM samples WHERE id=?1",
                            [id],
                            |row| row.get::<_, String>(0),
                        )
                        .map_err(|error| error.to_string())
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                (
                    output_type,
                    labels,
                    input_ids.iter().cloned().map(Some).collect(),
                )
            }
            "per_input_count" => {
                let output_type = execution
                    .get("outputType")
                    .and_then(Value::as_str)
                    .ok_or("outputType is required")?;
                let count = string_value(&values, "output_count")
                    .and_then(|value| value.parse::<usize>().ok())
                    .filter(|value| *value > 0 && *value <= 96)
                    .ok_or("Output count must be 1–96")?;
                let mut labels = Vec::with_capacity(input_ids.len() * count);
                let mut parents = Vec::with_capacity(input_ids.len() * count);
                for id in &input_ids {
                    let parent_label: String = tx
                        .query_row(
                            "SELECT coalesce(display_name,sample_code) FROM samples WHERE id=?1",
                            [id],
                            |row| row.get(0),
                        )
                        .map_err(|error| error.to_string())?;
                    for index in 1..=count {
                        labels.push(format!("{parent_label} · Output {index}"));
                        parents.push(Some(id.clone()));
                    }
                }
                (output_type, labels, parents)
            }
            "plate_or_dish" => {
                let kind =
                    string_value(&values, "container_type").ok_or("Container type is required")?;
                let output_type = if kind == "孔板" {
                    "PLATE"
                } else if kind == "培养皿" {
                    "DISH"
                } else {
                    return Err("Invalid container type".into());
                };
                let label = if output_type == "PLATE" {
                    string_value(&values, "plate_format")
                        .and_then(|format| {
                            crate::plate_layout::supported_capacity(format).map(|_| format)
                        })
                        .ok_or("Choose a supported plate format for a plate output")?
                } else {
                    kind
                };
                (
                    output_type,
                    vec![label.into()],
                    vec![input.as_ref().map(|(id, _)| id.clone())],
                )
            }
            "plate_wells" if input_type == Some("PLATE") => (
                "WELL",
                plate_assignments
                    .as_ref()
                    .expect("plate treatment assignments were validated")
                    .iter()
                    .map(|assignment| assignment.position.clone())
                    .collect(),
                vec![
                    input.as_ref().map(|(id, _)| id.clone());
                    plate_assignments.as_ref().map(Vec::len).unwrap_or(0)
                ],
            ),
            "plate_wells" => ("", Vec::new(), Vec::new()),
            "same_sample" => ("", Vec::new(), Vec::new()),
            "none" => ("", Vec::new(), Vec::new()),
            _ => return Err("Unsupported Protocol output mode".into()),
        };
    if !output_type.is_empty() {
        let registered: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sample_types WHERE canonical_type=upper(?1) AND archived_at IS NULL)",
                [output_type],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        if !registered {
            return Err(format!(
                "Output Sample type {output_type} is not registered"
            ));
        }
    }
    let mut output_ids = Vec::new();
    for (index, label) in output_labels.iter().enumerate() {
        let id = format!("sample-{record_id}-{index}");
        let code = next_sample_code(&tx, &experiment_id, &experiment_code, output_type)?;
        let parent_id = output_parents.get(index).and_then(Option::as_deref);
        let mut metadata = if let Some(parent) = parent_id {
            let inherited: String = tx
                .query_row(
                    "SELECT metadata_json FROM samples WHERE id=?1",
                    [parent],
                    |row| row.get(0),
                )
                .map_err(|error| error.to_string())?;
            serde_json::from_str::<Value>(&inherited)
                .ok()
                .and_then(|value| value.as_object().cloned())
                .unwrap_or_default()
        } else {
            Map::new()
        };
        if execution.get("engine").and_then(Value::as_str) != Some("sample_flow_v1") {
            metadata.extend(values.as_object().cloned().unwrap_or_else(Map::new));
        }
        if let Some(parent) = parent_id {
            metadata.insert("source_sample_id".into(), json!(parent));
        }
        metadata.insert("source_record_id".into(), json!(record_id));
        metadata.insert("source_protocol_id".into(), json!(protocol_id));
        metadata.insert("source_protocol_version".into(), json!(version));
        if output_type == "PLATE" {
            let capacity = string_value(&values, "plate_format")
                .and_then(crate::plate_layout::supported_capacity)
                .ok_or("Choose a supported plate format for a plate output")?;
            metadata.insert("plate_capacity".into(), json!(capacity));
        }
        if output_type == "WELL" {
            metadata.insert("well_position".into(), json!(label));
            if let Some(assignment) = plate_assignments
                .as_ref()
                .and_then(|assignments| assignments.get(index))
            {
                metadata.insert("treatment_factor".into(), json!(assignment.factor));
                metadata.insert("treatment_duration".into(), json!(assignment.duration));
                metadata.insert("treatment_group".into(), json!(assignment.group_index + 1));
                metadata.insert(
                    "source_plate_id".into(),
                    json!(input.as_ref().map(|(id, _)| id)),
                );
            }
        }
        insert_sample(
            &tx,
            &id,
            &experiment_id,
            &code,
            output_type,
            record_id,
            parent_id,
            label,
            task_date,
            &Value::Object(metadata),
        )?;
        tx.execute(
            "INSERT INTO event_outputs VALUES (?1,?2)",
            params![event_id, id],
        )
        .map_err(|error| error.to_string())?;
        tx.execute(
            "INSERT INTO record_samples VALUES (?1,?2,'output')",
            params![record_id, id],
        )
        .map_err(|error| error.to_string())?;
        output_ids.push(id);
    }
    if output_mode == "same_sample" {
        for id in &input_ids {
            tx.execute(
                "INSERT INTO event_outputs VALUES (?1,?2)",
                params![event_id, id],
            )
            .map_err(|error| error.to_string())?;
            tx.execute(
                "INSERT INTO record_samples VALUES (?1,?2,'output')",
                params![record_id, id],
            )
            .map_err(|error| error.to_string())?;
            output_ids.push(id.clone());
        }
    } else if output_ids.is_empty() && spec.get("terminalAssay").is_none() && output_mode != "none"
    {
        if let Some((id, _)) = &input {
            tx.execute(
                "INSERT INTO event_outputs VALUES (?1,?2)",
                params![event_id, id],
            )
            .map_err(|error| error.to_string())?;
        }
    }
    let output_summary = output_ids
        .iter()
        .map(|id| {
            tx.query_row("SELECT sample_code FROM samples WHERE id=?1", [id], |row| {
                row.get::<_, String>(0)
            })
            .map_err(|error| error.to_string())
        })
        .collect::<Result<Vec<_>, _>>()?
        .join("、");
    rendered = rendered.replace("{{output_sample_summary}}", &output_summary);
    let mut result_ids = Vec::new();
    for (index, result_type) in execution
        .get("resultTypes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .enumerate()
    {
        let id = format!("result-{record_id}-{index}");
        tx.execute(
            "INSERT INTO results (id,record_id,result_type,structured_data_json,created_at) VALUES (?1,?2,?3,?4,?5)",
            params![id, record_id, result_type, json!({"status":"pending"}).to_string(), task_date],
        )
        .map_err(|error| error.to_string())?;
        result_ids.push(id);
    }
    let data = json!({"title":title,"notes":"","inputs":input_ids,"outputs":output_ids,"results":result_ids,"values":values,"renderedContent":rendered});
    tx.execute(
        "UPDATE records SET current_data_json=?2 WHERE id=?1",
        params![record_id, data.to_string()],
    )
    .map_err(|error| error.to_string())?;
    tx.execute(
        "UPDATE tasks SET status='in_progress',record_id=?1,updated_at=?2 WHERE id=?3",
        params![record_id, task_date, task_id],
    )
    .map_err(|error| error.to_string())?;
    tx.commit().map_err(|error| error.to_string())?;
    Ok(ExecutionResult {
        task: json!({"id":task_id,"experimentId":experiment_id,"title":title,"status":"in_progress","recordId":record_id}),
        input_ids,
        output_ids,
        rendered_content: rendered,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        env, fs,
        sync::atomic::{AtomicU64, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };

    static DATABASE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    fn database() -> (Connection, std::path::PathBuf) {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let sequence = DATABASE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = env::temp_dir().join(format!(
            "labflow-protocol-execution-{}-{nonce}-{sequence}.sqlite",
            std::process::id()
        ));
        let connection = Connection::open(&path).unwrap();
        crate::apply_schema(&connection).unwrap();
        crate::ensure_builtin_protocols(&connection).unwrap();
        connection
            .execute(
                "INSERT INTO experiments VALUES ('exp','EXP900','Cell workflow','','#000')",
                [],
            )
            .unwrap();
        (connection, path)
    }
    fn task(db: &Connection, id: &str) {
        db.execute("INSERT INTO tasks (id,experiment_id,title,start_time,end_time,status,record_id,created_at,updated_at) VALUES (?1,'exp',?1,'2026-08-23T09:00:00','2026-08-23T10:00:00','planned',NULL,'now','now')",[id]).unwrap();
    }

    fn upstream_samples(db: &Connection, child_task_id: &str, samples: &[(&str, &str, &str)]) {
        task(db, "source-task");
        db.execute("INSERT INTO records (id,task_id,experiment_id,protocol_id,protocol_snapshot_json,current_data_json,updated_at) VALUES ('source-record','source-task','exp','pro-cell-treatment','{}','{}','now')", []).unwrap();
        db.execute(
            "UPDATE tasks SET record_id='source-record',status='completed' WHERE id='source-task'",
            [],
        )
        .unwrap();
        task(db, child_task_id);
        db.execute("INSERT INTO task_relations (id,experiment_id,parent_task_id,child_task_id,relation_type,created_at) VALUES (?1,'exp','source-task',?2,'depends_on','now')", params![format!("rel-{child_task_id}"), child_task_id]).unwrap();
        for (id, sample_type, metadata) in samples {
            db.execute("INSERT INTO samples (id,workspace_id,experiment_id,sample_code,sample_type,source_record_id,display_name,created_at,lineage_status,metadata_json) VALUES (?1,'local','exp',?2,?3,'source-record',?2,'now','complete',?4)", params![id, format!("EXP900-{sample_type}{}", &id[id.len()-1..]), sample_type, metadata]).unwrap();
            db.execute(
                "INSERT INTO record_samples VALUES ('source-record',?1,'output')",
                [id],
            )
            .unwrap();
        }
    }

    #[test]
    fn cell_protocol_chain_renders_and_survives_restart() {
        let (mut db, path) = database();
        task(&db, "thaw");
        let thaw = execute(
            &mut db,
            "thaw",
            "pro-cell-thaw",
            "r-thaw",
            json!({"cell_name":"A549"}),
            vec![],
        )
        .unwrap();
        assert!(thaw.rendered_content.contains("液氮中的 A549 冻存管"));
        let cell = thaw.output_ids[0].clone();
        task(&db, "passage");
        let passage = execute(
            &mut db,
            "passage",
            "pro-cell-passage",
            "r-passage",
            json!({"culture_mode":"贴壁","output_count":"2"}),
            vec![cell.clone()],
        )
        .unwrap();
        assert_eq!(passage.input_ids, vec![cell]);
        assert_eq!(passage.output_ids.len(), 2);
        assert!(passage.rendered_content.contains("PBS洗2～3次"));
        assert!(passage.rendered_content.contains("胞质回缩"));
        let consumed: i64 = db
            .query_row(
                "SELECT count(*) FROM sample_usages WHERE sample_id=?1 AND usage_type='consumed'",
                [&passage.input_ids[0]],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(consumed, 1);
        task(&db, "plate");
        let plate = execute(
            &mut db,
            "plate",
            "pro-cell-plating",
            "r-plate",
            json!({"container_type":"孔板","plate_format":"6孔板"}),
            vec![passage.output_ids[0].clone()],
        )
        .unwrap();
        let plate_id = plate.output_ids[0].clone();
        let plate_name: String = db
            .query_row(
                "SELECT display_name FROM samples WHERE id=?1",
                [&plate_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(plate_name, "6孔板");
        task(&db, "treatment");
        let treatment = execute(
            &mut db,
            "treatment",
            "pro-cell-treatment",
            "r-treatment",
            json!({"treatment_groups":"[{\"factor\":\"TNF-α\",\"duration\":\"24h\",\"wellCount\":2}]"}),
            vec![plate_id.clone()],
        )
        .unwrap();
        assert_eq!(treatment.output_ids.len(), 2);
        for well in &treatment.output_ids {
            let parent: String = db
                .query_row(
                    "SELECT parent_sample_id FROM samples WHERE id=?1",
                    [well],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(parent, plate_id);
        }
        let relations: i64 = db
            .query_row("SELECT count(*) FROM sample_relations", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(relations, 5);
        let current: String = db
            .query_row(
                "SELECT current_data_json FROM records WHERE id='r-treatment'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&current).unwrap()["outputs"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        drop(db);
        let reopened = Connection::open(&path).unwrap();
        let wells: i64 = reopened
            .query_row(
                "SELECT count(*) FROM samples WHERE sample_type='WELL'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(wells, 2);
        drop(reopened);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn cross_experiment_input_rolls_back() {
        let (mut db, path) = database();
        db.execute(
            "INSERT INTO experiments VALUES ('other','EXP901','Other','','#000')",
            [],
        )
        .unwrap();
        db.execute("INSERT INTO samples (id,experiment_id,sample_code,sample_type,display_name,created_at,metadata_json) VALUES ('foreign','other','EXP901-CELL01','CELL','Foreign','now','{}')",[]).unwrap();
        task(&db, "passage");
        assert!(execute(
            &mut db,
            "passage",
            "pro-cell-passage",
            "r-invalid",
            json!({"culture_mode":"贴壁","output_count":"1"}),
            vec!["foreign".into()]
        )
        .is_err());
        let records: i64 = db
            .query_row(
                "SELECT count(*) FROM records WHERE id='r-invalid'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let events: i64 = db
            .query_row(
                "SELECT count(*) FROM process_events WHERE record_id='r-invalid'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!((records, events), (0, 0));
        drop(db);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn consumed_passage_input_cannot_be_plated_again() {
        let (mut db, path) = database();
        task(&db, "thaw-consume");
        let thaw = execute(
            &mut db,
            "thaw-consume",
            "pro-cell-thaw",
            "r-thaw-consume",
            json!({"cell_name":"A549"}),
            vec![],
        )
        .unwrap();
        let original = thaw.output_ids[0].clone();
        task(&db, "passage-consume");
        execute(
            &mut db,
            "passage-consume",
            "pro-cell-passage",
            "r-passage-consume",
            json!({"culture_mode":"贴壁","output_count":"1"}),
            vec![original.clone()],
        )
        .unwrap();
        task(&db, "plate-consumed");
        let error = execute(
            &mut db,
            "plate-consumed",
            "pro-cell-plating",
            "r-plate-consumed",
            json!({"container_type":"孔板","plate_format":"6孔板"}),
            vec![original],
        )
        .unwrap_err();
        assert!(error.contains("must not be consumed"));
        let rolled_back: i64 = db
            .query_row(
                "SELECT count(*) FROM records WHERE id='r-plate-consumed'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(rolled_back, 0);
        drop(db);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn new_cell_object_and_dish_treatment_keep_sample_identity() {
        let (mut db, path) = database();
        task(&db, "passage-new");
        let passage = execute(
            &mut db,
            "passage-new",
            "pro-cell-passage",
            "r-passage-new",
            json!({"cell_name":"Primary cells","culture_mode":"悬浮","output_count":"1"}),
            vec![],
        )
        .unwrap();
        assert_eq!(passage.input_ids.len(), 1);
        assert!(passage.rendered_content.contains("1000 rpm，离心5 min"));
        assert!(passage.rendered_content.contains("直接传代法"));
        let (origin, source_record): (String, Option<String>) = db
            .query_row(
                "SELECT origin,source_record_id FROM samples WHERE id=?1",
                [&passage.input_ids[0]],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(origin, "external");
        assert!(source_record.is_none());
        let fake_imports: i64 = db
            .query_row(
                "SELECT count(*) FROM process_events WHERE id='event-r-passage-new-import'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(fake_imports, 0);
        task(&db, "dish");
        let dish = execute(
            &mut db,
            "dish",
            "pro-cell-plating",
            "r-dish",
            json!({"container_type":"培养皿"}),
            vec![passage.output_ids[0].clone()],
        )
        .unwrap();
        task(&db, "dish-treatment");
        let treatment = execute(
            &mut db,
            "dish-treatment",
            "pro-cell-treatment",
            "r-dish-treatment",
            json!({"treatment_type":"TNF-α"}),
            vec![dish.output_ids[0].clone()],
        )
        .unwrap();
        assert!(treatment.output_ids.is_empty());
        let same_identity: i64 = db.query_row("SELECT count(*) FROM event_outputs WHERE event_id='event-r-dish-treatment' AND sample_id=?1", [&dish.output_ids[0]], |row| row.get(0)).unwrap();
        assert_eq!(same_identity, 1);
        drop(db);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn grouped_six_well_treatment_creates_six_traced_samples() {
        let (mut db, path) = database();
        task(&db, "plate-groups");
        let plate = execute(
            &mut db,
            "plate-groups",
            "pro-cell-plating",
            "r-plate-groups",
            json!({"cell_name":"A549","container_type":"孔板","plate_format":"6孔板"}),
            vec![],
        )
        .unwrap();
        task(&db, "treatment-groups");
        let treatment = execute(
            &mut db,
            "treatment-groups",
            "pro-cell-treatment",
            "r-treatment-groups",
            json!({"treatment_groups":"[{\"factor\":\"si NC\",\"duration\":\"24h\",\"wellCount\":3},{\"factor\":\"si 123\",\"duration\":\"24h\",\"wellCount\":3}]"}),
            vec![plate.output_ids[0].clone()],
        )
        .unwrap();
        assert_eq!(treatment.output_ids.len(), 6);
        assert!(treatment
            .rendered_content
            .contains("si NC / 24h：A01, A02, A03"));
        assert!(treatment
            .rendered_content
            .contains("si 123 / 24h：B01, B02, B03"));
        let last_metadata: String = db
            .query_row(
                "SELECT metadata_json FROM samples WHERE id=?1",
                [&treatment.output_ids[5]],
                |row| row.get(0),
            )
            .unwrap();
        let metadata: Value = serde_json::from_str(&last_metadata).unwrap();
        assert_eq!(metadata["well_position"], "B03");
        assert_eq!(metadata["treatment_factor"], "si 123");
        assert_eq!(metadata["treatment_duration"], "24h");
        assert_eq!(metadata["source_plate_id"], plate.output_ids[0]);
        drop(db);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn over_capacity_plate_treatment_rolls_back_everything() {
        let (mut db, path) = database();
        task(&db, "plate-capacity");
        let plate = execute(
            &mut db,
            "plate-capacity",
            "pro-cell-plating",
            "r-plate-capacity",
            json!({"cell_name":"A549","container_type":"孔板","plate_format":"6孔板"}),
            vec![],
        )
        .unwrap();
        task(&db, "too-many-wells");
        let result = execute(
            &mut db,
            "too-many-wells",
            "pro-cell-treatment",
            "r-too-many-wells",
            json!({"treatment_groups":"[{\"factor\":\"si NC\",\"duration\":\"24h\",\"wellCount\":4},{\"factor\":\"si 123\",\"duration\":\"24h\",\"wellCount\":3}]"}),
            vec![plate.output_ids[0].clone()],
        );
        assert!(result.unwrap_err().contains("exceeding the 6-well plate"));
        let records: i64 = db
            .query_row(
                "SELECT count(*) FROM records WHERE id='r-too-many-wells'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let wells: i64 = db
            .query_row(
                "SELECT count(*) FROM samples WHERE source_record_id='r-too-many-wells'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!((records, wells), (0, 0));
        drop(db);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn current_builtin_protocol_versions_are_idempotent_on_restart() {
        let (db, path) = database();
        crate::ensure_builtin_protocols(&db).unwrap();
        crate::ensure_builtin_protocols(&db).unwrap();
        for protocol_id in [
            "pro-cell-plating",
            "pro-cell-treatment",
            "pro-rna",
            "pro-rt",
            "pro-qpcr",
            "pro-wb",
            "pro-supernatant",
            "pro-elisa",
            "pro-cck8",
        ] {
            let versions: i64 = db
                .query_row(
                    "SELECT count(*) FROM protocol_versions WHERE protocol_id=?1",
                    [protocol_id],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(versions, 1);
        }
        drop(db);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn root_task_can_register_external_inputs_without_fake_import_event() {
        let (mut db, path) = database();
        task(&db, "root-rna");
        let result = execute_with_external(
            &mut db,
            "root-rna",
            "pro-rna",
            "root-rna-record",
            json!({"resuspension_volume":"20","storage":"立即逆转录"}),
            vec![],
            vec![
                json!({"sampleType":"CELL","displayName":"siNC","metadata":{"existing_conditions":"siNC, 24 h"}}),
                json!({"sampleType":"CELL","displayName":"siARH","metadata":{"existing_conditions":"siARH, 24 h"}}),
            ],
        )
        .unwrap();

        assert_eq!(result.input_ids.len(), 2);
        assert_eq!(result.output_ids.len(), 2);
        for input_id in &result.input_ids {
            let (origin, source_record, parent): (String, Option<String>, Option<String>) = db
                .query_row(
                    "SELECT origin,source_record_id,parent_sample_id FROM samples WHERE id=?1",
                    [input_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .unwrap();
            assert_eq!(origin, "external");
            assert!(source_record.is_none());
            assert!(parent.is_none());
        }
        let fake_imports: i64 = db
            .query_row(
                "SELECT count(*) FROM process_events WHERE provenance='user_imported'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let task_relations: i64 = db
            .query_row("SELECT count(*) FROM task_relations", [], |row| row.get(0))
            .unwrap();
        assert_eq!((fake_imports, task_relations), (0, 0));
        for (index, output_id) in result.output_ids.iter().enumerate() {
            let (origin, parent): (String, String) = db
                .query_row(
                    "SELECT origin,parent_sample_id FROM samples WHERE id=?1",
                    [output_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .unwrap();
            assert_eq!(origin, "internal");
            assert_eq!(parent, result.input_ids[index]);
        }
        drop(db);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn experiment_sample_input_is_independent_of_task_relations_and_rolls_back() {
        let (mut db, path) = database();
        task(&db, "root-rna");
        db.execute("INSERT INTO samples (id,experiment_id,sample_code,sample_type,display_name,created_at,lineage_status,metadata_json,origin) VALUES ('existing-cell','exp','EXP900-CELL01','CELL','Existing cell','now','complete','{}','external')", []).unwrap();
        let result = execute(
            &mut db,
            "root-rna",
            "pro-rna",
            "root-rna-record",
            json!({"resuspension_volume":"20","storage":"立即逆转录"}),
            vec!["existing-cell".into()],
        )
        .unwrap();
        assert_eq!(result.input_ids, vec!["existing-cell"]);

        task(&db, "invalid-root-rna");
        let error = execute_with_external(
            &mut db,
            "invalid-root-rna",
            "pro-rna",
            "invalid-root-rna-record",
            json!({"resuspension_volume":"20","storage":"立即逆转录"}),
            vec![],
            vec![json!({"sampleType":"RNA","displayName":"Wrong type"})],
        )
        .unwrap_err();
        assert!(error.contains("does not accept RNA"));
        let leftovers: (i64, i64) = (
            db.query_row(
                "SELECT count(*) FROM records WHERE id='invalid-root-rna-record'",
                [],
                |row| row.get(0),
            )
            .unwrap(),
            db.query_row(
                "SELECT count(*) FROM samples WHERE id LIKE 'sample-invalid-root-rna-record-external-%'",
                [],
                |row| row.get(0),
            )
            .unwrap(),
        );
        assert_eq!(leftovers, (0, 0));
        drop(db);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn builtin_protocol_upgrade_never_downgrades_a_newer_schema() {
        let (db, path) = database();
        db.execute(
            "INSERT INTO protocol_versions (protocol_id,version_number,schema_json) VALUES ('pro-cell-passage',2,'{\"schemaVersion\":99}')",
            [],
        )
        .unwrap();
        db.execute(
            "UPDATE protocols SET active_version=2 WHERE id='pro-cell-passage'",
            [],
        )
        .unwrap();
        crate::ensure_builtin_protocols(&db).unwrap();
        let active: i64 = db
            .query_row(
                "SELECT active_version FROM protocols WHERE id='pro-cell-passage'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(active, 2);
        drop(db);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn rna_per_input_preserves_exact_lineage_and_blocks_double_consumption() {
        let (mut db, path) = database();
        upstream_samples(
            &db,
            "rna-task",
            &[
                (
                    "well-1",
                    "WELL",
                    r#"{"well_position":"A01","treatment_factor":"si NC","treatment_duration":"24h"}"#,
                ),
                (
                    "well-2",
                    "WELL",
                    r#"{"well_position":"A02","treatment_factor":"si 123","treatment_duration":"24h"}"#,
                ),
            ],
        );
        let rna = execute(
            &mut db,
            "rna-task",
            "pro-rna",
            "rna-record",
            json!({"resuspension_volume":"20","storage":"-80℃ 保存"}),
            vec!["well-1".into(), "well-2".into()],
        )
        .unwrap();
        assert_eq!(rna.output_ids.len(), 2);
        let outputs = rna
            .output_ids
            .iter()
            .map(|id| {
                db.query_row(
                    "SELECT sample_code,parent_sample_id,metadata_json FROM samples WHERE id=?1",
                    [id],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                        ))
                    },
                )
                .unwrap()
            })
            .collect::<Vec<_>>();
        assert_eq!(outputs[0].0, "EXP900-RNA01");
        assert_eq!(outputs[1].0, "EXP900-RNA02");
        assert_eq!(
            (outputs[0].1.as_str(), outputs[1].1.as_str()),
            ("well-1", "well-2")
        );
        assert_eq!(
            serde_json::from_str::<Value>(&outputs[1].2).unwrap()["treatment_factor"],
            "si 123"
        );

        task(&db, "wb-task");
        db.execute("INSERT INTO task_relations (id,experiment_id,parent_task_id,child_task_id,relation_type,created_at) VALUES ('rel-wb','exp','source-task','wb-task','depends_on','now')", []).unwrap();
        let error = execute(
            &mut db,
            "wb-task",
            "pro-wb",
            "wb-record",
            json!({"target_proteins":"GAPDH","gel_percentage":"10","primary_antibody":"GAPDH 1:1000","secondary_antibody":"1:5000","exposure_time":"1 min"}),
            vec!["well-1".into()],
        )
        .unwrap_err();
        assert!(error.contains("consumed"));
        let rolled_back: i64 = db
            .query_row(
                "SELECT count(*) FROM records WHERE id='wb-record'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(rolled_back, 0);
        drop(db);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn supernatant_is_non_destructive_and_elisa_creates_setup_not_samples_or_results() {
        let (mut db, path) = database();
        upstream_samples(
            &db,
            "sup-task",
            &[(
                "well-1",
                "WELL",
                r#"{"well_position":"A01","treatment_factor":"LPS"}"#,
            )],
        );
        let supernatant = execute(
            &mut db,
            "sup-task",
            "pro-supernatant",
            "sup-record",
            json!({"collection_time":"24h","collection_volume":"500","storage":"-80℃ 保存"}),
            vec!["well-1".into()],
        )
        .unwrap();
        assert_eq!(supernatant.output_ids.len(), 1);
        let code: String = db
            .query_row(
                "SELECT sample_code FROM samples WHERE id=?1",
                [&supernatant.output_ids[0]],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(code, "EXP900-SUP01");

        task(&db, "rna-after-sup");
        db.execute("INSERT INTO task_relations (id,experiment_id,parent_task_id,child_task_id,relation_type,created_at) VALUES ('rel-rna-after-sup','exp','source-task','rna-after-sup','depends_on','now')", []).unwrap();
        execute(
            &mut db,
            "rna-after-sup",
            "pro-rna",
            "rna-after-sup-record",
            json!({"resuspension_volume":"20","storage":"立即逆转录"}),
            vec!["well-1".into()],
        )
        .unwrap();

        task(&db, "elisa-task");
        db.execute("INSERT INTO task_relations (id,experiment_id,parent_task_id,child_task_id,relation_type,created_at) VALUES ('rel-elisa','exp','sup-task','elisa-task','depends_on','now')", []).unwrap();
        let elisa = execute(
            &mut db,
            "elisa-task",
            "pro-elisa",
            "elisa-record",
            json!({"assay_items":"IL-4, IL-10","sample_dilution":"1","reference_wavelength":"570 nm"}),
            supernatant.output_ids,
        )
        .unwrap();
        assert!(elisa.output_ids.is_empty());
        let result_count: i64 = db
            .query_row(
                "SELECT count(*) FROM results WHERE record_id='elisa-record'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let item_count: i64 = db
            .query_row(
                "SELECT count(*) FROM assay_items WHERE record_id='elisa-record'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!((result_count, item_count), (0, 2));
        drop(db);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn western_blot_creates_protein_sample_and_image_result() {
        let (mut db, path) = database();
        upstream_samples(
            &db,
            "wb-task",
            &[(
                "well-1",
                "WELL",
                r#"{"well_position":"B01","treatment_factor":"si 123"}"#,
            )],
        );
        let wb = execute(
            &mut db,
            "wb-task",
            "pro-wb",
            "wb-record",
            json!({"target_proteins":"TIAM1, GAPDH","gel_percentage":"10","primary_antibody":"TIAM1 1:1000","secondary_antibody":"1:5000","exposure_time":"1 min"}),
            vec!["well-1".into()],
        )
        .unwrap();
        assert_eq!(wb.output_ids.len(), 1);
        let (code, parent): (String, String) = db
            .query_row(
                "SELECT sample_code,parent_sample_id FROM samples WHERE id=?1",
                [&wb.output_ids[0]],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(
            (code.as_str(), parent.as_str()),
            ("EXP900-PROTEIN01", "well-1")
        );
        let result_type: String = db
            .query_row(
                "SELECT result_type FROM results WHERE record_id='wb-record'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(result_type, "western_blot_image");
        assert!(wb.rendered_content.contains("TIAM1, GAPDH"));
        drop(db);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn reverse_transcription_and_qpcr_keep_terminal_assay_out_of_sample_lineage() {
        let (mut db, path) = database();
        upstream_samples(&db, "rt-task", &[("rna-1", "RNA", "{}")]);
        let rt = execute(
            &mut db,
            "rt-task",
            "pro-rt",
            "rt-record",
            json!({"rna_amount":"1.0","extra_reactions":"2"}),
            vec!["rna-1".into()],
        )
        .unwrap();
        let (cdna_code, cdna_type): (String, String) = db
            .query_row(
                "SELECT sample_code,sample_type FROM samples WHERE id=?1",
                [&rt.output_ids[0]],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(cdna_code, "EXP900-cDNA01");
        assert_eq!(cdna_type, "CDNA");

        task(&db, "qpcr-task");
        db.execute("INSERT INTO task_relations (id,experiment_id,parent_task_id,child_task_id,relation_type,created_at) VALUES ('rel-qpcr','exp','rt-task','qpcr-task','depends_on','now')", []).unwrap();
        let qpcr = execute(
            &mut db,
            "qpcr-task",
            "pro-qpcr",
            "qpcr-record",
            json!({"assay_items":"Actin, GAPDH, ARH"}),
            rt.output_ids,
        )
        .unwrap();
        assert!(qpcr.output_ids.is_empty());
        let terminal_state: (i64, i64, i64) = db
            .query_row(
                "SELECT
                   (SELECT count(*) FROM results WHERE record_id='qpcr-record'),
                   (SELECT count(*) FROM assay_items WHERE record_id='qpcr-record'),
                   (SELECT count(*) FROM event_outputs WHERE event_id='event-qpcr-record')",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(terminal_state, (0, 3, 0));
        drop(db);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn generic_sample_flow_inherits_parent_metadata_without_copying_record_fields() {
        let (mut db, path) = database();
        upstream_samples(
            &db,
            "custom-task",
            &[("rna-custom", "RNA", r#"{"group":"siNC","time":"24h"}"#)],
        );
        db.execute("INSERT INTO protocols (id,name,category,active_version,accent,description,origin) VALUES ('custom-rt','Custom RT','自定义',1,'#000','','user')", []).unwrap();
        db.execute(
            "INSERT INTO protocol_versions (protocol_id,version_number,schema_json,origin,created_at) VALUES ('custom-rt',1,?1,'user','now')",
            [json!({
                "schemaVersion":1,
                "fields":[{"key":"operator_note","label":"Note","kind":"text"}],
                "template":"日期：{{date}}\n输入：{{input_sample_summary}}\n输出：{{output_sample_summary}}\n{{operator_note}}",
                "execution":{"engine":"sample_flow_v1","eventType":"custom:rt","inputSource":"parent_task_outputs","inputCardinality":"many","inputTypes":["RNA"],"outputType":"CDNA","outputMode":"per_input","consumptionPolicy":"consume","metadataPolicy":"inherit_parent"}
            }).to_string()],
        ).unwrap();
        let result = execute(
            &mut db,
            "custom-task",
            "custom-rt",
            "custom-record",
            json!({"operator_note":"kept only in Record"}),
            vec!["rna-custom".into()],
        )
        .unwrap();
        let metadata_json: String = db
            .query_row(
                "SELECT metadata_json FROM samples WHERE id=?1",
                [&result.output_ids[0]],
                |row| row.get(0),
            )
            .unwrap();
        let metadata: Value = serde_json::from_str(&metadata_json).unwrap();
        assert_eq!(metadata["group"], "siNC");
        assert_eq!(metadata["time"], "24h");
        assert!(metadata.get("operator_note").is_none());
        assert_eq!(metadata["source_sample_id"], "rna-custom");
        assert_eq!(metadata["source_protocol_id"], "custom-rt");
        assert!(result.rendered_content.contains("EXP900-RNA"));
        assert!(result.rendered_content.contains("EXP900-cDNA01"));
        let consumed: i64 = db
            .query_row(
                "SELECT count(*) FROM sample_usages WHERE sample_id='rna-custom' AND usage_type='consumed'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(consumed, 1);
        drop(db);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn generic_multiple_and_measurement_flows_preserve_lineage_semantics() {
        let (mut db, path) = database();
        upstream_samples(&db, "multiple-task", &[("cell-custom", "CELL", "{}")]);
        db.execute(
            "INSERT INTO sample_types VALUES ('TISSUE','Tissue','user','now',NULL)",
            [],
        )
        .unwrap();
        db.execute("INSERT INTO protocols (id,name,category,active_version,accent,description,origin) VALUES ('custom-multiple','Split','自定义',1,'#000','','user')", []).unwrap();
        db.execute("INSERT INTO protocol_versions (protocol_id,version_number,schema_json,origin,created_at) VALUES ('custom-multiple',1,?1,'user','now')", [json!({"fields":[{"key":"output_count","label":"Count","kind":"number","required":true}],"template":"{{input_sample_summary}} -> {{output_sample_summary}}","execution":{"engine":"sample_flow_v1","eventType":"custom:split","inputSource":"parent_task_outputs","inputCardinality":"many","inputTypes":["CELL"],"outputType":"TISSUE","outputMode":"per_input_count","consumptionPolicy":"non_destructive"}}).to_string()]).unwrap();
        let split = execute(
            &mut db,
            "multiple-task",
            "custom-multiple",
            "multiple-record",
            json!({"output_count":"3"}),
            vec!["cell-custom".into()],
        )
        .unwrap();
        assert_eq!(split.output_ids.len(), 3);
        let inherited: i64 = db.query_row("SELECT count(*) FROM samples WHERE source_record_id='multiple-record' AND parent_sample_id='cell-custom' AND sample_type='TISSUE'",[],|row|row.get(0)).unwrap();
        assert_eq!(inherited, 3);

        task(&db, "measurement-task");
        db.execute("INSERT INTO task_relations (id,experiment_id,parent_task_id,child_task_id,relation_type,created_at) VALUES ('rel-measure','exp','source-task','measurement-task','depends_on','now')", []).unwrap();
        db.execute("INSERT INTO protocols (id,name,category,active_version,accent,description,origin) VALUES ('custom-measure','Measure','自定义',1,'#000','','user')", []).unwrap();
        db.execute("INSERT INTO protocol_versions (protocol_id,version_number,schema_json,origin,created_at) VALUES ('custom-measure',1,?1,'user','now')", [json!({"fields":[],"template":"Measurement {{input_sample_summary}}","execution":{"engine":"sample_flow_v1","eventType":"custom:measure","inputSource":"parent_task_outputs","inputCardinality":"many","inputTypes":["CELL"],"outputMode":"none","consumptionPolicy":"non_destructive"}}).to_string()]).unwrap();
        let measured = execute(
            &mut db,
            "measurement-task",
            "custom-measure",
            "measurement-record",
            json!({}),
            vec!["cell-custom".into()],
        )
        .unwrap();
        assert!(measured.output_ids.is_empty());
        let event_outputs: i64 = db
            .query_row(
                "SELECT count(*) FROM event_outputs WHERE event_id='event-measurement-record'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(event_outputs, 0);
        drop(db);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn user_activated_builtin_template_version_survives_restart_catalog_sync() {
        let (db, path) = database();
        db.execute("INSERT INTO protocol_versions (protocol_id,version_number,schema_json,origin,created_at) VALUES ('pro-rt',2,'{\"schemaVersion\":2,\"template\":\"User text\"}','user','now')", []).unwrap();
        db.execute(
            "UPDATE protocols SET active_version=2 WHERE id='pro-rt'",
            [],
        )
        .unwrap();
        crate::ensure_builtin_protocols(&db).unwrap();
        let (active, origin, template): (i64, String, String) = db.query_row("SELECT p.active_version,pv.origin,json_extract(pv.schema_json,'$.template') FROM protocols p JOIN protocol_versions pv ON pv.protocol_id=p.id AND pv.version_number=p.active_version WHERE p.id='pro-rt'",[],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?))).unwrap();
        assert_eq!(
            (active, origin.as_str(), template.as_str()),
            (2, "user", "User text")
        );
        drop(db);
        fs::remove_file(path).unwrap();
    }
}
