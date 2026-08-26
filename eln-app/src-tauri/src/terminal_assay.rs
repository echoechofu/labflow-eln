use calamine::{Reader, Xlsx};
use rusqlite::{params, Connection};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    fmt::Write as FmtWrite,
    fs,
    io::{Cursor, Write},
    path::Path,
};

const MAX_RAW_FILE_SIZE: usize = 20 * 1024 * 1024;

fn required_string<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .ok_or_else(|| format!("Missing {key}"))
}

fn plate_capacity(model: &str) -> Result<usize, String> {
    crate::plate_layout::supported_capacity(model)
        .ok_or_else(|| "Plate model must be 6, 12, 24, 48, 96, or 384 wells".to_string())
}

fn validate_well(model: &str, position: &str) -> Result<String, String> {
    let normalized = position.trim().to_uppercase();
    let capacity = plate_capacity(model)?;
    if crate::plate_layout::well_positions(capacity).contains(&normalized) {
        Ok(normalized)
    } else {
        Err(format!(
            "Well {position} is not valid for a {model}-well plate"
        ))
    }
}

pub fn create_items(
    connection: &Connection,
    record_id: &str,
    raw_items: &str,
) -> Result<(), String> {
    let mut seen = HashSet::new();
    let items = raw_items
        .split([',', '，', '\n'])
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .filter(|item| seen.insert(item.to_lowercase()))
        .collect::<Vec<_>>();
    if items.is_empty() {
        return Err("Add at least one assay item".into());
    }
    for (position, item) in items.iter().enumerate() {
        connection
            .execute(
                "INSERT INTO assay_items (id,record_id,display_name,position,metadata_json) VALUES (?1,?2,?3,?4,'{}')",
                params![format!("assay-item-{record_id}-{position}"), record_id, item, position as i64],
            )
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

pub fn create_plate(connection: &Connection, request: &Value) -> Result<(), String> {
    let id = required_string(request, "id")?;
    let record_id = required_string(request, "recordId")?;
    let name = required_string(request, "name")?;
    let model = required_string(request, "plateModel")?;
    plate_capacity(model)?;
    let created_at = required_string(request, "createdAt")?;
    connection
        .execute(
            "INSERT INTO assay_plates (id,record_id,name,plate_model,created_at)
             SELECT ?1,?2,?3,?4,?5 WHERE EXISTS(SELECT 1 FROM records WHERE id=?2)",
            params![id, record_id, name, model, created_at],
        )
        .map_err(|error| error.to_string())
        .and_then(|changed| {
            if changed == 1 {
                Ok(())
            } else {
                Err("Record not found".into())
            }
        })
}

pub fn delete_empty_plate(connection: &Connection, plate_id: &str) -> Result<(), String> {
    let changed = connection
        .execute(
            "DELETE FROM assay_plates
             WHERE id=?1
               AND NOT EXISTS(SELECT 1 FROM assay_well_mappings WHERE plate_id=?1)
               AND NOT EXISTS(SELECT 1 FROM assay_raw_imports WHERE plate_id=?1)",
            [plate_id],
        )
        .map_err(|error| error.to_string())?;
    if changed == 1 {
        return Ok(());
    }
    let exists: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM assay_plates WHERE id=?1)",
            [plate_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if exists {
        Err("Only a Plate without Mapping or Raw Result can be deleted".into())
    } else {
        Err("Assay plate not found".into())
    }
}

pub fn replace_mappings(
    connection: &mut Connection,
    plate_id: &str,
    mappings: &[Value],
    changed_at: &str,
) -> Result<(), String> {
    let tx = connection
        .transaction()
        .map_err(|error| error.to_string())?;
    let (record_id, model): (String, String) = tx
        .query_row(
            "SELECT record_id,plate_model FROM assay_plates WHERE id=?1",
            [plate_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|_| "Assay plate not found".to_string())?;
    let mut wells = HashSet::new();
    let mut validated = Vec::with_capacity(mappings.len());
    for mapping in mappings {
        let id = required_string(mapping, "id")?;
        let well = validate_well(&model, required_string(mapping, "wellPosition")?)?;
        if !wells.insert(well.clone()) {
            return Err(format!("Well {well} is assigned more than once"));
        }
        let sample_id = required_string(mapping, "sampleId")?;
        let item_id = required_string(mapping, "assayItemId")?;
        let sample_allowed: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM record_samples WHERE record_id=?1 AND sample_id=?2 AND role='input')",
                params![record_id, sample_id],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        if !sample_allowed {
            return Err("Every mapped Sample must be an input of this Record".into());
        }
        let item_allowed: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM assay_items WHERE id=?1 AND record_id=?2)",
                params![item_id, record_id],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        if !item_allowed {
            return Err("Every mapped assay item must belong to this Record".into());
        }
        validated.push((
            id.to_owned(),
            well,
            sample_id.to_owned(),
            item_id.to_owned(),
        ));
    }
    tx.execute(
        "DELETE FROM assay_well_mappings WHERE plate_id=?1",
        [plate_id],
    )
    .map_err(|error| error.to_string())?;
    for (id, well, sample_id, item_id) in validated {
        tx.execute(
            "INSERT INTO assay_well_mappings (id,plate_id,well_position,sample_id,assay_item_id,assignment_role,metadata_json,created_at) VALUES (?1,?2,?3,?4,?5,'measurement','{}',?6)",
            params![id, plate_id, well, sample_id, item_id, changed_at],
        )
        .map_err(|error| error.to_string())?;
    }
    tx.commit().map_err(|error| error.to_string())
}

fn parse_delimited(input: &str, delimiter: char) -> Result<Vec<Vec<String>>, String> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = input.trim_start_matches('\u{feff}').chars().peekable();
    while let Some(character) = chars.next() {
        match character {
            '"' if quoted && chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            value if value == delimiter && !quoted => {
                row.push(field.trim().to_owned());
                field.clear();
            }
            '\n' if !quoted => {
                row.push(field.trim_end_matches('\r').trim().to_owned());
                field.clear();
                if row.iter().any(|value| !value.is_empty()) {
                    rows.push(std::mem::take(&mut row));
                } else {
                    row.clear();
                }
            }
            value => field.push(value),
        }
    }
    if quoted {
        return Err("Raw result contains an unterminated quoted field".into());
    }
    row.push(field.trim_end_matches('\r').trim().to_owned());
    if row.iter().any(|value| !value.is_empty()) {
        rows.push(row);
    }
    if rows.len() < 2 {
        return Err("Raw result must contain a header and at least one data row".into());
    }
    Ok(rows)
}

fn delimiter_for(input: &str) -> char {
    let header = input.lines().next().unwrap_or("");
    if header.matches('\t').count() > header.matches(',').count() {
        '\t'
    } else {
        ','
    }
}

#[derive(Debug)]
struct ParsedMeasurement {
    well: String,
    numeric: Option<f64>,
    text: String,
    raw: Value,
}

fn parse_measurements(
    bytes: &[u8],
    model: &str,
    well_column: &str,
    measurement_column: &str,
) -> Result<Vec<ParsedMeasurement>, String> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| "Raw result must be a UTF-8 CSV or TSV file".to_string())?;
    let rows = parse_delimited(text, delimiter_for(text))?;
    let headers = rows[0].clone();
    let column = |name: &str| {
        headers
            .iter()
            .position(|header| header.eq_ignore_ascii_case(name))
            .ok_or_else(|| format!("Raw result column not found: {name}"))
    };
    let well_index = column(well_column)?;
    let measurement_index = column(measurement_column)?;
    let mut seen = HashSet::new();
    let mut parsed = Vec::new();
    for row in rows.into_iter().skip(1) {
        let Some(well_value) = row.get(well_index).filter(|value| !value.is_empty()) else {
            continue;
        };
        let well = validate_well(model, well_value)?;
        if !seen.insert(well.clone()) {
            return Err(format!("Raw result contains duplicate well {well}"));
        }
        let value = row
            .get(measurement_index)
            .map(String::as_str)
            .unwrap_or("")
            .trim()
            .to_owned();
        if value.is_empty() {
            continue;
        }
        let raw = headers
            .iter()
            .enumerate()
            .map(|(index, header)| {
                (
                    header.clone(),
                    Value::String(row.get(index).cloned().unwrap_or_default()),
                )
            })
            .collect::<Map<_, _>>();
        parsed.push(ParsedMeasurement {
            well,
            numeric: value.parse::<f64>().ok(),
            text: value,
            raw: Value::Object(raw),
        });
    }
    if parsed.is_empty() {
        return Err("Raw result contains no usable measurements".into());
    }
    Ok(parsed)
}

fn parse_xlsx_measurements(
    bytes: &[u8],
    model: &str,
    well_column: &str,
    measurement_column: &str,
) -> Result<Vec<ParsedMeasurement>, String> {
    let mut workbook = Xlsx::new(Cursor::new(bytes))
        .map_err(|error| format!("Cannot open qPCR Excel result: {error}"))?;
    for sheet_name in workbook.sheet_names().to_owned() {
        let range = workbook
            .worksheet_range(&sheet_name)
            .map_err(|error| format!("Cannot read Excel sheet {sheet_name}: {error}"))?;
        let rows = range.rows().collect::<Vec<_>>();
        let Some((header_index, headers)) = rows.iter().enumerate().find_map(|(index, row)| {
            let values = row.iter().map(ToString::to_string).collect::<Vec<_>>();
            let has_well = values
                .iter()
                .any(|value| value.trim().eq_ignore_ascii_case(well_column));
            let has_measurement = values
                .iter()
                .any(|value| value.trim().eq_ignore_ascii_case(measurement_column));
            (has_well && has_measurement).then_some((index, values))
        }) else {
            continue;
        };
        let well_index = headers
            .iter()
            .position(|header| header.trim().eq_ignore_ascii_case(well_column))
            .unwrap();
        let measurement_index = headers
            .iter()
            .position(|header| header.trim().eq_ignore_ascii_case(measurement_column))
            .unwrap();
        let mut seen = HashSet::new();
        let mut parsed = Vec::new();
        for row in rows.into_iter().skip(header_index + 1) {
            let well_value = row
                .get(well_index)
                .map(ToString::to_string)
                .unwrap_or_default();
            if well_value.trim().is_empty() {
                continue;
            }
            let well = validate_well(model, &well_value)?;
            if !seen.insert(well.clone()) {
                return Err(format!("Raw result contains duplicate well {well}"));
            }
            let text = row
                .get(measurement_index)
                .map(ToString::to_string)
                .unwrap_or_default();
            if text.trim().is_empty() {
                continue;
            }
            let raw = headers
                .iter()
                .enumerate()
                .filter(|(_, header)| !header.trim().is_empty())
                .map(|(index, header)| {
                    (
                        header.clone(),
                        Value::String(row.get(index).map(ToString::to_string).unwrap_or_default()),
                    )
                })
                .collect::<Map<_, _>>();
            parsed.push(ParsedMeasurement {
                well,
                numeric: text.trim().parse::<f64>().ok(),
                text: text.trim().to_owned(),
                raw: Value::Object(raw),
            });
        }
        if parsed.is_empty() {
            return Err("Excel result contains no usable measurements".into());
        }
        return Ok(parsed);
    }
    Err(format!(
        "No Excel sheet contains both {well_column} and {measurement_column} columns"
    ))
}

pub fn upload_raw(
    connection: &mut Connection,
    files_dir: &Path,
    request: &Value,
    bytes: &[u8],
) -> Result<Value, String> {
    if bytes.is_empty() || bytes.len() > MAX_RAW_FILE_SIZE {
        return Err("Raw result file must be between 1 byte and 20 MB".into());
    }
    let id = required_string(request, "id")?;
    let record_id = required_string(request, "recordId")?;
    let plate_id = required_string(request, "plateId")?;
    let attachment_id = required_string(request, "attachmentId")?;
    let file_name = required_string(request, "fileName")?;
    if file_name.contains(['/', '\\']) || matches!(file_name, "." | "..") {
        return Err("Raw result filename is invalid".into());
    }
    let metric_key = required_string(request, "metricKey")?;
    let well_column = required_string(request, "wellColumn")?;
    let measurement_column = required_string(request, "measurementColumn")?;
    let imported_at = required_string(request, "importedAt")?;
    let (plate_record_id, model): (String, String) = connection
        .query_row(
            "SELECT record_id,plate_model FROM assay_plates WHERE id=?1",
            [plate_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|_| "Assay plate not found".to_string())?;
    if plate_record_id != record_id {
        return Err("Assay plate does not belong to this Record".into());
    }
    let measurements = if file_name.to_ascii_lowercase().ends_with(".xlsx") {
        parse_xlsx_measurements(bytes, &model, well_column, measurement_column)?
    } else {
        parse_measurements(bytes, &model, well_column, measurement_column)?
    };
    let content_sha256 = format!("{:x}", Sha256::digest(bytes));
    let directory = files_dir.join(attachment_id);
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let path = directory.join(file_name);
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|error| {
            format!("Raw result attachment already exists or cannot be created: {error}")
        })?;
    file.write_all(bytes).map_err(|error| error.to_string())?;
    let relative_path = format!("files/{attachment_id}/{file_name}");
    let transaction_result = (|| {
        let tx = connection
            .transaction()
            .map_err(|error| error.to_string())?;
        tx.execute(
            "INSERT INTO attachments (id,record_id,file_name,relative_path,mime_type,size,created_at) VALUES (?1,?2,?3,?4,?5,?6,?7)",
            params![attachment_id, record_id, file_name, relative_path, request.get("mimeType").and_then(Value::as_str), bytes.len() as i64, imported_at],
        )
        .map_err(|error| error.to_string())?;
        tx.execute(
            "INSERT INTO assay_raw_imports (id,record_id,plate_id,attachment_id,metric_key,well_column,measurement_column,content_sha256,imported_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![id, record_id, plate_id, attachment_id, metric_key, well_column, measurement_column, content_sha256, imported_at],
        )
        .map_err(|error| error.to_string())?;
        for (index, measurement) in measurements.iter().enumerate() {
            tx.execute(
                "INSERT INTO assay_raw_measurements (id,import_id,well_position,metric_key,numeric_value,text_value,raw_row_json) VALUES (?1,?2,?3,?4,?5,?6,?7)",
                params![format!("measurement-{id}-{index}"), id, measurement.well, metric_key, measurement.numeric, measurement.text, measurement.raw.to_string()],
            )
            .map_err(|error| error.to_string())?;
        }
        tx.commit().map_err(|error| error.to_string())
    })();
    if let Err(error) = transaction_result {
        let _ = fs::remove_file(&path);
        return Err(error);
    }
    Ok(json!({
        "id": id,
        "attachmentId": attachment_id,
        "relativePath": relative_path,
        "contentSha256": content_sha256,
        "measurementCount": measurements.len()
    }))
}

#[derive(Debug)]
struct QpcrWell {
    measurement_id: String,
    sample_id: String,
    sample_code: String,
    item_id: String,
    value: f64,
}

fn qpcr_wells(connection: &Connection, record_id: &str) -> Result<Vec<QpcrWell>, String> {
    let mut statement = connection
        .prepare(
            "SELECT measurement.id,mapping.sample_id,sample.sample_code,mapping.assay_item_id,measurement.numeric_value
             FROM assay_raw_measurements measurement
             JOIN assay_raw_imports import ON import.id=measurement.import_id
             JOIN assay_well_mappings mapping ON mapping.plate_id=import.plate_id AND mapping.well_position=measurement.well_position
             JOIN samples sample ON sample.id=mapping.sample_id
             WHERE import.record_id=?1 AND lower(measurement.metric_key)='cq' AND measurement.numeric_value IS NOT NULL",
        )
        .map_err(|error| error.to_string())?;
    let wells = statement
        .query_map([record_id], |row| {
            Ok(QpcrWell {
                measurement_id: row.get(0)?,
                sample_id: row.get(1)?,
                sample_code: row.get(2)?,
                item_id: row.get(3)?,
                value: row.get(4)?,
            })
        })
        .map_err(|error| error.to_string())?
        .map(|row| row.map_err(|error| error.to_string()))
        .collect();
    wells
}

fn string_array(value: &Value, key: &str) -> Result<Vec<String>, String> {
    value
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("Missing {key}"))?
        .iter()
        .map(|item| {
            item.as_str()
                .map(str::to_owned)
                .ok_or_else(|| format!("{key} must contain strings"))
        })
        .collect()
}

fn mean(values: &[f64]) -> f64 {
    values.iter().sum::<f64>() / values.len() as f64
}

fn item_name(connection: &Connection, item_id: &str) -> String {
    connection
        .query_row(
            "SELECT display_name FROM assay_items WHERE id=?1",
            [item_id],
            |row| row.get(0),
        )
        .unwrap_or_else(|_| item_id.to_owned())
}

fn delta_ct_text(
    connection: &Connection,
    name: &str,
    created_at: &str,
    included_count: usize,
    result: &Value,
) -> String {
    let mut text = format!(
        "qPCR ΔCt Analysis\n分析名称：{name}\n保存时间：{created_at}\n纳入 Cq Well：{included_count}\n"
    );
    for combination in result["combinations"].as_array().into_iter().flatten() {
        let target = item_name(
            connection,
            combination["targetItemId"].as_str().unwrap_or(""),
        );
        let reference = item_name(
            connection,
            combination["referenceItemId"].as_str().unwrap_or(""),
        );
        let _ = write!(text, "\n目的基因：{target}\n内参基因：{reference}\n");
        for sample in combination["samples"].as_array().into_iter().flatten() {
            let _ = writeln!(
                text,
                "{}\nTarget mean Cq：{:.3}\nReference mean Cq：{:.3}\nΔCt：{:.3}\n",
                sample["sampleCode"].as_str().unwrap_or(""),
                sample["targetMeanCq"].as_f64().unwrap_or(f64::NAN),
                sample["referenceMeanCq"].as_f64().unwrap_or(f64::NAN),
                sample["deltaCt"].as_f64().unwrap_or(f64::NAN)
            );
        }
    }
    text.trim_end().to_owned()
}

fn delta_delta_ct_text(
    connection: &Connection,
    name: &str,
    created_at: &str,
    control_mode: &str,
    result: &Value,
) -> String {
    let mut text = format!(
        "qPCR ΔΔCt Analysis\n分析名称：{name}\n保存时间：{created_at}\n对照方式：{}\n",
        if control_mode == "shared" {
            "共同对照"
        } else {
            "各实验组对应对照"
        }
    );
    for combination in result["combinations"].as_array().into_iter().flatten() {
        let target = item_name(
            connection,
            combination["targetItemId"].as_str().unwrap_or(""),
        );
        let reference = item_name(
            connection,
            combination["referenceItemId"].as_str().unwrap_or(""),
        );
        let _ = write!(text, "\n目的基因：{target}\n内参基因：{reference}\n");
        for sample in combination["samples"].as_array().into_iter().flatten() {
            let _ = writeln!(
                text,
                "{}\n实验组：{}\n对照组：{}\nΔCt：{:.3}\nControl mean ΔCt：{:.3}\nΔΔCt：{:.3}\nRelative expression：{:.3}\n",
                sample["sampleCode"].as_str().unwrap_or(""),
                sample["group"].as_str().unwrap_or(""),
                sample["controlGroup"].as_str().unwrap_or(""),
                sample["deltaCt"].as_f64().unwrap_or(f64::NAN),
                sample["controlMeanDeltaCt"].as_f64().unwrap_or(f64::NAN),
                sample["deltaDeltaCt"].as_f64().unwrap_or(f64::NAN),
                sample["relativeExpression"].as_f64().unwrap_or(f64::NAN)
            );
        }
    }
    text.trim_end().to_owned()
}

fn append_record_analysis_section(
    connection: &Connection,
    record_id: &str,
    analysis_id: &str,
    kind: &str,
    title: &str,
    text: &str,
    saved_at: &str,
) -> Result<(), String> {
    let raw: String = connection
        .query_row(
            "SELECT current_data_json FROM records WHERE id=?1",
            [record_id],
            |row| row.get(0),
        )
        .map_err(|_| "Record not found".to_string())?;
    let mut current: Value = serde_json::from_str(&raw).map_err(|error| error.to_string())?;
    let old_sections = current
        .get("analysisSections")
        .cloned()
        .unwrap_or_else(|| json!([]));
    let sections = current
        .as_object_mut()
        .ok_or("Record data must be a JSON object")?
        .entry("analysisSections")
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .ok_or("Record analysisSections must be an array")?;
    sections.push(json!({
        "id":analysis_id,
        "kind":kind,
        "title":title,
        "text":text,
        "savedAt":saved_at
    }));
    let new_sections = current["analysisSections"].clone();
    connection
        .execute(
            "UPDATE records SET current_data_json=?2,updated_at=?3 WHERE id=?1",
            params![record_id, current.to_string(), saved_at],
        )
        .map_err(|error| error.to_string())?;
    connection
        .execute(
            "INSERT INTO record_changes (id,record_id,field_path,old_value_json,new_value_json,changed_at) VALUES (?1,?2,'analysisSections',?3,?4,?5)",
            params![format!("record-change-{analysis_id}"), record_id, old_sections.to_string(), new_sections.to_string(), saved_at],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn create_delta_ct_analysis(
    connection: &mut Connection,
    request: &Value,
) -> Result<Value, String> {
    let id = required_string(request, "id")?;
    let record_id = required_string(request, "recordId")?;
    let name = required_string(request, "name")?;
    let created_at = required_string(request, "createdAt")?;
    let targets = string_array(request, "targetItemIds")?;
    let references = string_array(request, "referenceItemIds")?;
    let included_ids = string_array(request, "includedMeasurementIds")?;
    if targets.is_empty() || references.is_empty() {
        return Err("Select at least one target and one reference gene".into());
    }
    let targets = targets.into_iter().collect::<HashSet<_>>();
    let references = references.into_iter().collect::<HashSet<_>>();
    if !targets.is_disjoint(&references) {
        return Err("A gene cannot be both target and reference in one analysis".into());
    }
    let item_count: i64 = connection
        .query_row(
            "SELECT count(*) FROM assay_items WHERE record_id=?1 AND id IN (SELECT value FROM json_each(?2))",
            params![record_id, serde_json::to_string(&targets.iter().collect::<Vec<_>>()).unwrap()],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    let selected_item_count = targets.len() + references.len();
    let reference_count: i64 = connection
        .query_row(
            "SELECT count(*) FROM assay_items WHERE record_id=?1 AND id IN (SELECT value FROM json_each(?2))",
            params![record_id, serde_json::to_string(&references.iter().collect::<Vec<_>>()).unwrap()],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if item_count as usize + reference_count as usize != selected_item_count {
        return Err("Every selected gene must belong to this qPCR Record".into());
    }
    let included = included_ids.into_iter().collect::<HashSet<_>>();
    if included.is_empty() {
        return Err("Include at least one numeric Cq well".into());
    }
    let wells = qpcr_wells(connection, record_id)?;
    if !included
        .iter()
        .all(|id| wells.iter().any(|well| &well.measurement_id == id))
    {
        return Err("Included wells must be numeric Cq measurements from this Record".into());
    }
    let mut grouped: HashMap<(String, String), (String, Vec<f64>)> = HashMap::new();
    for well in wells
        .iter()
        .filter(|well| included.contains(&well.measurement_id))
    {
        grouped
            .entry((well.sample_id.clone(), well.item_id.clone()))
            .or_insert_with(|| (well.sample_code.clone(), Vec::new()))
            .1
            .push(well.value);
    }
    let mut combinations = Vec::new();
    for target in &targets {
        for reference in &references {
            let mut samples = Vec::new();
            let sample_ids = grouped
                .keys()
                .map(|(sample_id, _)| sample_id)
                .collect::<HashSet<_>>();
            for sample_id in sample_ids {
                let Some((sample_code, target_values)) =
                    grouped.get(&(sample_id.clone(), target.clone()))
                else {
                    continue;
                };
                let Some((_, reference_values)) =
                    grouped.get(&(sample_id.clone(), reference.clone()))
                else {
                    continue;
                };
                let target_mean = mean(target_values);
                let reference_mean = mean(reference_values);
                samples.push(json!({
                    "sampleId": sample_id,
                    "sampleCode": sample_code,
                    "targetMeanCq": target_mean,
                    "referenceMeanCq": reference_mean,
                    "targetReplicateCount": target_values.len(),
                    "referenceReplicateCount": reference_values.len(),
                    "deltaCt": target_mean - reference_mean
                }));
            }
            samples.sort_by(|left, right| {
                left["sampleCode"]
                    .as_str()
                    .cmp(&right["sampleCode"].as_str())
            });
            combinations
                .push(json!({"targetItemId":target,"referenceItemId":reference,"samples":samples}));
        }
    }
    let config = json!({
        "targetItemIds": targets,
        "referenceItemIds": references,
        "includedMeasurementIds": included,
        "qcNotes": request.get("qcNotes").cloned().unwrap_or_else(|| json!({}))
    });
    let mut result = json!({"combinations":combinations});
    let rendered_text = delta_ct_text(connection, name, created_at, included.len(), &result);
    result["renderedText"] = Value::String(rendered_text.clone());
    let tx = connection
        .transaction()
        .map_err(|error| error.to_string())?;
    tx
        .execute(
            "INSERT INTO qpcr_delta_ct_analyses (id,record_id,name,config_json,result_json,created_at) VALUES (?1,?2,?3,?4,?5,?6)",
            params![id, record_id, name, config.to_string(), result.to_string(), created_at],
        )
        .map_err(|error| error.to_string())?;
    append_record_analysis_section(
        &tx,
        record_id,
        id,
        "delta_ct",
        name,
        &rendered_text,
        created_at,
    )?;
    tx.commit().map_err(|error| error.to_string())?;
    Ok(
        json!({"id":id,"recordId":record_id,"name":name,"config":config,"result":result,"createdAt":created_at}),
    )
}

pub fn create_delta_delta_ct_analysis(
    connection: &mut Connection,
    request: &Value,
) -> Result<Value, String> {
    let id = required_string(request, "id")?;
    let record_id = required_string(request, "recordId")?;
    let delta_ct_analysis_id = required_string(request, "deltaCtAnalysisId")?;
    let name = required_string(request, "name")?;
    let reference_item_id = required_string(request, "referenceItemId")?;
    let control_mode = required_string(request, "controlMode")?;
    let created_at = required_string(request, "createdAt")?;
    if !matches!(control_mode, "shared" | "matched") {
        return Err("Control mode must be shared or matched".into());
    }
    let (source_config_raw, source_result_raw): (String, String) = connection
        .query_row(
            "SELECT config_json,result_json FROM qpcr_delta_ct_analyses WHERE id=?1 AND record_id=?2",
            params![delta_ct_analysis_id, record_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|_| "ΔCt analysis not found for this Record".to_string())?;
    let source_config: Value =
        serde_json::from_str(&source_config_raw).map_err(|error| error.to_string())?;
    if !source_config["referenceItemIds"]
        .as_array()
        .is_some_and(|items| {
            items
                .iter()
                .any(|item| item.as_str() == Some(reference_item_id))
        })
    {
        return Err("Choose one reference gene from the selected ΔCt analysis".into());
    }
    let sample_groups = request
        .get("sampleGroups")
        .and_then(Value::as_object)
        .ok_or("Missing sampleGroups")?;
    let shared_control = request
        .get("sharedControlGroup")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    let control_relations = request
        .get("controlRelations")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    if control_mode == "shared" && shared_control.is_empty() {
        return Err("Select a shared control group".into());
    }
    let source_result: Value =
        serde_json::from_str(&source_result_raw).map_err(|error| error.to_string())?;
    let combinations = source_result["combinations"]
        .as_array()
        .ok_or("Invalid ΔCt result")?;
    let mut output = Vec::new();
    for combination in combinations
        .iter()
        .filter(|item| item["referenceItemId"].as_str() == Some(reference_item_id))
    {
        let samples = combination["samples"]
            .as_array()
            .ok_or("Invalid ΔCt samples")?;
        if samples.iter().any(|sample| {
            sample["sampleId"]
                .as_str()
                .and_then(|sample_id| sample_groups.get(sample_id))
                .and_then(Value::as_str)
                .map(str::trim)
                .is_none_or(str::is_empty)
        }) {
            return Err("Assign every ΔCt Sample to a group before ΔΔCt analysis".into());
        }
        let mut control_means: HashMap<String, f64> = HashMap::new();
        let groups = samples
            .iter()
            .filter_map(|sample| {
                let sample_id = sample["sampleId"].as_str()?;
                let group = sample_groups.get(sample_id)?.as_str()?.trim();
                let delta = sample["deltaCt"].as_f64()?;
                (!group.is_empty()).then_some((group.to_owned(), delta))
            })
            .fold(
                HashMap::<String, Vec<f64>>::new(),
                |mut values, (group, delta)| {
                    values.entry(group).or_default().push(delta);
                    values
                },
            );
        for (group, values) in groups {
            control_means.insert(group, mean(&values));
        }
        let mut calculated = Vec::new();
        for sample in samples {
            let Some(sample_id) = sample["sampleId"].as_str() else {
                continue;
            };
            let Some(group) = sample_groups
                .get(sample_id)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
            else {
                continue;
            };
            let control_group = if control_mode == "shared" {
                shared_control
            } else {
                control_relations
                    .get(group)
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .unwrap_or("")
            };
            let Some(control_mean) = control_means.get(control_group) else {
                return Err(format!(
                    "Control group {control_group} has no ΔCt values for a selected Target"
                ));
            };
            let delta_ct = sample["deltaCt"].as_f64().ok_or("Invalid ΔCt value")?;
            let delta_delta_ct = delta_ct - control_mean;
            calculated.push(json!({
                "sampleId":sample_id,
                "sampleCode":sample["sampleCode"],
                "group":group,
                "controlGroup":control_group,
                "deltaCt":delta_ct,
                "controlMeanDeltaCt":control_mean,
                "deltaDeltaCt":delta_delta_ct,
                "relativeExpression":2_f64.powf(-delta_delta_ct)
            }));
        }
        output.push(json!({
            "targetItemId":combination["targetItemId"],
            "referenceItemId":reference_item_id,
            "samples":calculated
        }));
    }
    if output.is_empty() {
        return Err("The selected ΔCt analysis has no results for this reference gene".into());
    }
    let config = json!({
        "referenceItemId":reference_item_id,
        "controlMode":control_mode,
        "sampleGroups":sample_groups,
        "sharedControlGroup":shared_control,
        "controlRelations":control_relations
    });
    let mut result = json!({"combinations":output});
    let rendered_text = delta_delta_ct_text(connection, name, created_at, control_mode, &result);
    result["renderedText"] = Value::String(rendered_text.clone());
    let tx = connection
        .transaction()
        .map_err(|error| error.to_string())?;
    tx.execute(
        "INSERT INTO qpcr_delta_delta_ct_analyses (id,record_id,delta_ct_analysis_id,name,config_json,result_json,created_at) VALUES (?1,?2,?3,?4,?5,?6,?7)",
        params![id,record_id,delta_ct_analysis_id,name,config.to_string(),result.to_string(),created_at],
    ).map_err(|error| error.to_string())?;
    append_record_analysis_section(
        &tx,
        record_id,
        id,
        "delta_delta_ct",
        name,
        &rendered_text,
        created_at,
    )?;
    tx.commit().map_err(|error| error.to_string())?;
    Ok(
        json!({"id":id,"recordId":record_id,"deltaCtAnalysisId":delta_ct_analysis_id,"name":name,"config":config,"result":result,"createdAt":created_at}),
    )
}

pub fn workspace(connection: &Connection, record_id: &str) -> Result<Value, String> {
    let exists: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM records WHERE id=?1)",
            [record_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if !exists {
        return Err("Record not found".into());
    }
    fn json_rows(connection: &Connection, sql: &str, id: &str) -> Result<Vec<Value>, String> {
        let mut statement = connection.prepare(sql).map_err(|error| error.to_string())?;
        let rows = statement
            .query_map([id], |row| {
                let raw: String = row.get(0)?;
                Ok(serde_json::from_str(&raw).unwrap_or(json!({})))
            })
            .map_err(|error| error.to_string())?
            .map(|row| row.map_err(|error| error.to_string()))
            .collect();
        rows
    }
    let items = json_rows(connection, "SELECT json_object('id',id,'displayName',display_name,'position',position,'metadata',json(metadata_json)) FROM assay_items WHERE record_id=?1 ORDER BY position", record_id)?;
    let plates = json_rows(connection, "SELECT json_object('id',id,'name',name,'plateModel',plate_model,'createdAt',created_at) FROM assay_plates WHERE record_id=?1 ORDER BY created_at,id", record_id)?;
    let mappings = json_rows(connection, "SELECT json_object('id',mapping.id,'plateId',mapping.plate_id,'wellPosition',mapping.well_position,'sampleId',mapping.sample_id,'assayItemId',mapping.assay_item_id,'assignmentRole',mapping.assignment_role,'metadata',json(mapping.metadata_json)) FROM assay_well_mappings mapping JOIN assay_plates plate ON plate.id=mapping.plate_id WHERE plate.record_id=?1 ORDER BY mapping.plate_id,mapping.well_position", record_id)?;
    let imports = json_rows(connection, "SELECT json_object('id',import.id,'plateId',import.plate_id,'attachmentId',import.attachment_id,'fileName',attachment.file_name,'relativePath',attachment.relative_path,'metricKey',import.metric_key,'wellColumn',import.well_column,'measurementColumn',import.measurement_column,'contentSha256',import.content_sha256,'importedAt',import.imported_at,'measurementCount',(SELECT count(*) FROM assay_raw_measurements WHERE import_id=import.id)) FROM assay_raw_imports import JOIN attachments attachment ON attachment.id=import.attachment_id WHERE import.record_id=?1 ORDER BY import.imported_at,import.id", record_id)?;
    let joined = json_rows(connection, "SELECT json_object('mappingId',mapping.id,'measurementId',measurement.id,'plateId',plate.id,'plateName',plate.name,'wellPosition',mapping.well_position,'sampleId',mapping.sample_id,'sampleCode',sample.sample_code,'assayItemId',item.id,'assayItem',item.display_name,'importId',import.id,'fileName',attachment.file_name,'metricKey',measurement.metric_key,'numericValue',measurement.numeric_value,'textValue',measurement.text_value) FROM assay_well_mappings mapping JOIN assay_plates plate ON plate.id=mapping.plate_id JOIN assay_items item ON item.id=mapping.assay_item_id JOIN samples sample ON sample.id=mapping.sample_id JOIN assay_raw_imports import ON import.plate_id=plate.id JOIN assay_raw_measurements measurement ON measurement.import_id=import.id AND measurement.well_position=mapping.well_position JOIN attachments attachment ON attachment.id=import.attachment_id WHERE plate.record_id=?1 ORDER BY plate.created_at,import.imported_at,mapping.well_position", record_id)?;
    let delta_ct_analyses = json_rows(connection, "SELECT json_object('id',id,'recordId',record_id,'name',name,'config',json(config_json),'result',json(result_json),'createdAt',created_at) FROM qpcr_delta_ct_analyses WHERE record_id=?1 ORDER BY created_at,id", record_id)?;
    let delta_delta_ct_analyses = json_rows(connection, "SELECT json_object('id',id,'recordId',record_id,'deltaCtAnalysisId',delta_ct_analysis_id,'name',name,'config',json(config_json),'result',json(result_json),'createdAt',created_at) FROM qpcr_delta_delta_ct_analyses WHERE record_id=?1 ORDER BY created_at,id", record_id)?;
    Ok(
        json!({"items":items,"plates":plates,"mappings":mappings,"imports":imports,"joinedWells":joined,"deltaCtAnalyses":delta_ct_analyses,"deltaDeltaCtAnalyses":delta_delta_ct_analyses}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn csv_parser_supports_quotes_and_preserves_non_numeric_values() {
        let rows = parse_measurements(
            b"Well,Cq,Note\nA01,17.506,ok\nA02,Undetermined,\"no, signal\"\n",
            "96",
            "Well",
            "Cq",
        )
        .unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].numeric, Some(17.506));
        assert_eq!(rows[1].numeric, None);
        assert_eq!(rows[1].text, "Undetermined");
    }

    #[test]
    fn raw_parser_rejects_invalid_and_duplicate_wells() {
        assert!(parse_measurements(b"Well,OD\nZ99,1.0\n", "96", "Well", "OD").is_err());
        assert!(
            parse_measurements(b"Well,OD\nA01,1.0\nA01,1.1\n", "96", "Well", "OD")
                .unwrap_err()
                .contains("duplicate well")
        );
    }

    #[test]
    fn mapping_and_raw_import_join_persist_without_creating_samples() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "labflow-terminal-assay-{}-{nonce}",
            std::process::id()
        ));
        let files = root.join("files");
        fs::create_dir_all(&files).unwrap();
        let database_path = root.join("labflow.sqlite");
        let mut connection = Connection::open(&database_path).unwrap();
        crate::apply_schema(&connection).unwrap();
        connection
            .execute(
                "INSERT INTO experiments VALUES ('exp','EXP777','Assay','','#000')",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO protocols (id,name,category,active_version,accent) VALUES ('protocol','Terminal','Assay',1,'#000')",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO protocol_versions (protocol_id,version_number,schema_json) VALUES ('protocol',1,'{}')",
                [],
            )
            .unwrap();
        connection.execute("INSERT INTO tasks (id,experiment_id,title,start_time,end_time,status,record_id,created_at,updated_at) VALUES ('task','exp','qPCR','2026-08-25T09:00','2026-08-25T10:00','in_progress','record','now','now')", []).unwrap();
        connection
            .execute(
                "INSERT INTO records VALUES ('record','task','exp','protocol','{}','{}','now')",
                [],
            )
            .unwrap();
        for (id, code) in [("s1", "EXP777-cDNA01"), ("s2", "EXP777-cDNA02")] {
            connection.execute("INSERT INTO samples (id,experiment_id,sample_code,sample_type,display_name,created_at,metadata_json) VALUES (?1,'exp',?2,'CDNA',?2,'now','{}')", params![id,code]).unwrap();
            connection
                .execute(
                    "INSERT INTO record_samples VALUES ('record',?1,'input')",
                    [id],
                )
                .unwrap();
        }
        create_items(&connection, "record", "Actin, GAPDH, ARH").unwrap();
        create_plate(&connection, &json!({"id":"plate","recordId":"record","name":"Plate 1","plateModel":"96","createdAt":"now"})).unwrap();
        replace_mappings(
            &mut connection,
            "plate",
            &[
                json!({"id":"m1","wellPosition":"A01","sampleId":"s1","assayItemId":"assay-item-record-0"}),
                json!({"id":"m2","wellPosition":"A02","sampleId":"s1","assayItemId":"assay-item-record-1"}),
                json!({"id":"m3","wellPosition":"A03","sampleId":"s1","assayItemId":"assay-item-record-2"}),
                json!({"id":"m4","wellPosition":"A04","sampleId":"s2","assayItemId":"assay-item-record-0"}),
                json!({"id":"m5","wellPosition":"A05","sampleId":"s2","assayItemId":"assay-item-record-1"}),
                json!({"id":"m6","wellPosition":"A06","sampleId":"s2","assayItemId":"assay-item-record-2"}),
            ],
            "now",
        )
        .unwrap();
        let result = upload_raw(
            &mut connection,
            &files,
            &json!({"id":"import","recordId":"record","plateId":"plate","attachmentId":"attachment","fileName":"raw.csv","mimeType":"text/csv","metricKey":"cq","wellColumn":"Well","measurementColumn":"Cq","importedAt":"now"}),
            b"Well,Cq\nA01,17.5\nA02,18.0\nA03,22.1\nA04,18.0\nA05,18.5\nA06,23.0\nA07,Undetermined\n",
        )
        .unwrap();
        assert_eq!(result["measurementCount"], 7);
        let snapshot = workspace(&connection, "record").unwrap();
        assert_eq!(snapshot["joinedWells"].as_array().unwrap().len(), 6);
        let delta = create_delta_ct_analysis(
            &mut connection,
            &json!({
                "id":"delta",
                "recordId":"record",
                "name":"Actin ΔCt",
                "targetItemIds":["assay-item-record-2"],
                "referenceItemIds":["assay-item-record-0","assay-item-record-1"],
                "includedMeasurementIds":["measurement-import-0","measurement-import-1","measurement-import-2","measurement-import-3","measurement-import-4","measurement-import-5"],
                "qcNotes":{},
                "createdAt":"now"
            }),
        )
        .unwrap();
        assert_eq!(delta["result"]["combinations"].as_array().unwrap().len(), 2);
        let delta_delta = create_delta_delta_ct_analysis(
            &mut connection,
            &json!({
                "id":"delta-delta",
                "recordId":"record",
                "deltaCtAnalysisId":"delta",
                "name":"Relative expression",
                "referenceItemId":"assay-item-record-0",
                "controlMode":"shared",
                "sampleGroups":{"s1":"control","s2":"treated"},
                "sharedControlGroup":"control",
                "controlRelations":{},
                "createdAt":"now"
            }),
        )
        .unwrap();
        let treated = &delta_delta["result"]["combinations"][0]["samples"][1];
        assert!((treated["relativeExpression"].as_f64().unwrap() - 2_f64.powf(-0.4)).abs() < 1e-9);
        let record_data: String = connection
            .query_row(
                "SELECT current_data_json FROM records WHERE id='record'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let record_data: Value = serde_json::from_str(&record_data).unwrap();
        assert_eq!(record_data["analysisSections"].as_array().unwrap().len(), 2);
        assert!(record_data["analysisSections"][0]["text"]
            .as_str()
            .unwrap()
            .contains("Target mean Cq"));
        let store = crate::read_store(&connection).unwrap();
        assert_eq!(
            store["records"][0]["analysisSections"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert!(delete_empty_plate(&connection, "plate").is_err());
        create_plate(&connection, &json!({"id":"empty-plate","recordId":"record","name":"Wrong plate","plateModel":"96","createdAt":"now"})).unwrap();
        delete_empty_plate(&connection, "empty-plate").unwrap();
        let sample_count: i64 = connection
            .query_row("SELECT count(*) FROM samples", [], |row| row.get(0))
            .unwrap();
        assert_eq!(sample_count, 2);
        drop(connection);

        let reopened = Connection::open(&database_path).unwrap();
        crate::apply_schema(&reopened).unwrap();
        assert_eq!(
            workspace(&reopened, "record").unwrap()["joinedWells"]
                .as_array()
                .unwrap()
                .len(),
            6
        );
        assert_eq!(
            workspace(&reopened, "record").unwrap()["deltaCtAnalyses"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert!(files.join("attachment/raw.csv").exists());
        drop(reopened);
        fs::remove_dir_all(root).unwrap();
    }
}
