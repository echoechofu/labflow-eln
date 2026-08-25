use rusqlite::{params, Connection};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::{collections::HashSet, fs, io::Write, path::Path};

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
    let measurements = parse_measurements(bytes, &model, well_column, measurement_column)?;
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
    Ok(
        json!({"items":items,"plates":plates,"mappings":mappings,"imports":imports,"joinedWells":joined}),
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
                "INSERT INTO protocols VALUES ('protocol','Terminal','Assay',1,'#000')",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO protocol_versions VALUES ('protocol',1,'{}')",
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
        create_items(&connection, "record", "Actin, ARH").unwrap();
        create_plate(&connection, &json!({"id":"plate","recordId":"record","name":"Plate 1","plateModel":"96","createdAt":"now"})).unwrap();
        replace_mappings(
            &mut connection,
            "plate",
            &[
                json!({"id":"m1","wellPosition":"A01","sampleId":"s1","assayItemId":"assay-item-record-0"}),
                json!({"id":"m2","wellPosition":"A02","sampleId":"s1","assayItemId":"assay-item-record-1"}),
            ],
            "now",
        )
        .unwrap();
        let result = upload_raw(
            &mut connection,
            &files,
            &json!({"id":"import","recordId":"record","plateId":"plate","attachmentId":"attachment","fileName":"raw.csv","mimeType":"text/csv","metricKey":"cq","wellColumn":"Well","measurementColumn":"Cq","importedAt":"now"}),
            b"Well,Cq\nA01,17.5\nA02,22.1\nA03,30.0\n",
        )
        .unwrap();
        assert_eq!(result["measurementCount"], 3);
        let snapshot = workspace(&connection, "record").unwrap();
        assert_eq!(snapshot["joinedWells"].as_array().unwrap().len(), 2);
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
            2
        );
        assert!(files.join("attachment/raw.csv").exists());
        drop(reopened);
        fs::remove_dir_all(root).unwrap();
    }
}
