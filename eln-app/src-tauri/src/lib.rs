use rusqlite::{params, Connection};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};
use tauri::{AppHandle, Manager, State};
mod lineage;
mod plate_layout;
mod protocol_catalog;
mod protocol_execution;
mod task_graph;
mod terminal_assay;
mod workspace_backup;

struct DatabaseState(Mutex<Connection>);

/// Canonical user-data root. `data_dir()` supplies the OS base directory; the
/// stable product folder intentionally does not depend on the bundle identifier.
fn canonical_app_data_dir(platform_data_dir: PathBuf) -> PathBuf {
    platform_data_dir.join("LabFlow")
}

fn app_data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .data_dir()
        .map(canonical_app_data_dir)
        .map_err(|error| error.to_string())
}

fn database_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app_data_dir(app)?.join("labflow.sqlite"))
}
fn attachments_dir(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app_data_dir(app)?.join("files"))
}

fn initialize_database(app: &AppHandle) -> Result<Connection, String> {
    fs::create_dir_all(attachments_dir(app)?).map_err(|error| error.to_string())?;
    let path = database_path(app)?;
    let connection = Connection::open(path).map_err(|error| error.to_string())?;
    apply_schema(&connection)?;
    ensure_builtin_protocols(&connection)?;
    let mut connection = connection;
    seed_if_empty(&mut connection)?;
    Ok(connection)
}

fn ensure_builtin_protocols(connection: &Connection) -> Result<(), String> {
    for builtin in protocol_catalog::builtins() {
        let id = builtin.id;
        let schema = builtin.schema;
        let exists: Option<i64> = connection
            .query_row(
                "SELECT active_version FROM protocols WHERE id=?1",
                [id],
                |row| row.get(0),
            )
            .ok();
        if exists.is_none() {
            connection.execute("INSERT INTO protocols (id,name,category,active_version,accent,origin) VALUES (?1,?2,?3,1,?4,'builtin')",params![id,builtin.name,builtin.category,builtin.accent]).map_err(|e|e.to_string())?;
            connection.execute("INSERT INTO protocol_versions (protocol_id,version_number,schema_json,origin,created_at) VALUES (?1,1,?2,'builtin',datetime('now'))",params![id,schema]).map_err(|e|e.to_string())?;
        } else {
            let latest: String = connection.query_row("SELECT schema_json FROM protocol_versions WHERE protocol_id=?1 AND origin='builtin' ORDER BY version_number DESC LIMIT 1",[id],|row|row.get(0)).map_err(|e|e.to_string())?;
            let active_schema_version = serde_json::from_str::<Value>(&latest)
                .ok()
                .and_then(|schema| schema.get("schemaVersion").and_then(Value::as_i64))
                .unwrap_or(0);
            if active_schema_version < builtin.schema_version {
                let next: i64 = connection
                    .query_row(
                        "SELECT coalesce(max(version_number),0)+1 FROM protocol_versions WHERE protocol_id=?1",
                        [id],
                        |row| row.get(0),
                    )
                    .map_err(|error| error.to_string())?;
                connection.execute("INSERT INTO protocol_versions (protocol_id,version_number,schema_json,origin,created_at) VALUES (?1,?2,?3,'builtin',datetime('now'))",params![id,next,schema]).map_err(|e|e.to_string())?;
                let active_origin: String = connection.query_row("SELECT pv.origin FROM protocols p JOIN protocol_versions pv ON pv.protocol_id=p.id AND pv.version_number=p.active_version WHERE p.id=?1",[id],|row|row.get(0)).map_err(|e|e.to_string())?;
                if active_origin == "builtin" {
                    connection
                        .execute(
                            "UPDATE protocols SET active_version=?2 WHERE id=?1",
                            params![id, next],
                        )
                        .map_err(|e| e.to_string())?;
                }
            }
        }
    }
    connection.execute("UPDATE protocols SET name='细胞复苏' WHERE id='pro-cell-thaw' AND name='细胞复苏 — A549'",[]).map_err(|error|error.to_string())?;
    connection.execute("UPDATE protocols SET name='细胞传代' WHERE id='pro-cell-passage' AND name='细胞传代 — adherent cells'",[]).map_err(|error|error.to_string())?;
    Ok(())
}

fn ensure_column(
    connection: &Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> Result<(), String> {
    let mut statement = connection
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(|error| error.to_string())?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| error.to_string())?;
    if columns.filter_map(Result::ok).any(|name| name == column) {
        return Ok(());
    }
    connection
        .execute_batch(&format!("ALTER TABLE {table} ADD COLUMN {definition}"))
        .map_err(|error| error.to_string())
}

fn apply_schema(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch("PRAGMA foreign_keys=ON;")
        .map_err(|error| error.to_string())?;
    connection
        .execute_batch(include_str!("schema.sql"))
        .map_err(|error| error.to_string())?;
    ensure_column(connection, "samples", "display_name", "display_name TEXT")?;
    ensure_column(connection, "samples", "created_at", "created_at TEXT")?;
    ensure_column(
        connection,
        "samples",
        "lineage_status",
        "lineage_status TEXT NOT NULL DEFAULT 'complete'",
    )?;
    ensure_column(
        connection,
        "samples",
        "metadata_json",
        "metadata_json TEXT NOT NULL DEFAULT '{}'",
    )?;
    ensure_column(connection, "samples", "archived_at", "archived_at TEXT")?;
    ensure_column(
        connection,
        "samples",
        "origin",
        "origin TEXT NOT NULL DEFAULT 'internal' CHECK(origin IN ('internal','external'))",
    )?;
    ensure_column(
        connection,
        "process_events",
        "archived_at",
        "archived_at TEXT",
    )?;
    ensure_column(connection, "tasks", "created_at", "created_at TEXT")?;
    ensure_column(connection, "tasks", "updated_at", "updated_at TEXT")?;
    ensure_column(
        connection,
        "protocols",
        "description",
        "description TEXT NOT NULL DEFAULT ''",
    )?;
    ensure_column(
        connection,
        "protocols",
        "origin",
        "origin TEXT NOT NULL DEFAULT 'builtin'",
    )?;
    ensure_column(
        connection,
        "protocol_versions",
        "origin",
        "origin TEXT NOT NULL DEFAULT 'builtin'",
    )?;
    ensure_column(
        connection,
        "protocol_versions",
        "created_at",
        "created_at TEXT",
    )?;
    connection
        .execute("UPDATE samples SET sample_type=upper(sample_type)", [])
        .map_err(|error| error.to_string())?;
    for (canonical_type, display_name) in [
        ("CELL", "CELL"),
        ("PLATE", "PLATE"),
        ("DISH", "DISH"),
        ("WELL", "WELL"),
        ("RNA", "RNA"),
        ("CDNA", "cDNA"),
        ("PROTEIN", "PROTEIN"),
        ("SUP", "SUP"),
    ] {
        connection.execute(
            "INSERT OR IGNORE INTO sample_types (canonical_type,display_name,origin,created_at) VALUES (?1,?2,'builtin',datetime('now'))",
            params![canonical_type, display_name],
        ).map_err(|error| error.to_string())?;
    }
    connection.execute("INSERT OR IGNORE INTO sample_types (canonical_type,display_name,origin,created_at) SELECT DISTINCT upper(sample_type), upper(sample_type), 'user', datetime('now') FROM samples", []).map_err(|error| error.to_string())?;
    connection.execute_batch("UPDATE samples SET origin='external' WHERE id IN (SELECT output.sample_id FROM process_events event JOIN event_outputs output ON output.event_id=event.id WHERE event.provenance='user_imported'); UPDATE samples SET origin='external' WHERE source_record_id IS NULL AND parent_sample_id IS NULL AND NOT EXISTS (SELECT 1 FROM event_outputs output JOIN process_events event ON event.id=output.event_id WHERE output.sample_id=samples.id AND event.provenance='labflow_recorded');").map_err(|error| error.to_string())?;
    connection.execute_batch("UPDATE samples SET lineage_status='partial' WHERE id IN (SELECT eo.sample_id FROM process_events event JOIN event_outputs eo ON eo.event_id=event.id JOIN samples output ON output.id=eo.sample_id WHERE (event.event_type IN ('passage','plating') AND NOT EXISTS (SELECT 1 FROM event_inputs WHERE event_id=event.id)) OR (event.event_type='treatment' AND upper(output.sample_type)='WELL' AND NOT EXISTS (SELECT 1 FROM event_inputs input JOIN samples source ON source.id=input.sample_id WHERE input.event_id=event.id AND upper(source.sample_type)='PLATE')));").map_err(|error| error.to_string())?;
    connection.execute_batch("INSERT OR IGNORE INTO sample_usages (event_id,sample_id,usage_type,created_at) SELECT event.id,input.sample_id,'consumed',event.created_at FROM process_events event JOIN event_inputs input ON input.event_id=event.id WHERE event.event_type='passage' AND NOT EXISTS (SELECT 1 FROM sample_usages usage WHERE usage.event_id=event.id AND usage.sample_id=input.sample_id);").map_err(|error| error.to_string())?;
    Ok(())
}

fn seed_if_empty(connection: &mut Connection) -> Result<(), String> {
    let count: i64 = connection
        .query_row("SELECT count(*) FROM experiments", [], |row| row.get(0))
        .map_err(|error| error.to_string())?;
    if count > 0 {
        return Ok(());
    }
    ensure_builtin_protocols(connection)?;
    let transaction = connection
        .transaction()
        .map_err(|error| error.to_string())?;
    transaction.execute(
        "INSERT INTO experiments VALUES ('exp-template','EXP001','A549 siRNA 筛选模板','模板工作流：复苏 → 传代 → 铺板 → 刺激/成像 → RNA','#167c80')",
        [],
    ).map_err(|error| error.to_string())?;
    transaction.execute(
        "INSERT OR IGNORE INTO protocols (id,name,category,active_version,accent) VALUES ('pro-rna','RNA Extraction — Trizol','分子生物学',1,'#6957e8')",
        [],
    ).map_err(|error| error.to_string())?;
    transaction.execute(
        "INSERT OR IGNORE INTO protocol_versions (protocol_id,version_number,schema_json) VALUES ('pro-rna',1,?1)",
        [r#"{"schemaVersion":1,"blocks":["选择上级 Task","选择输入 Sample","RNA 提取","输出 RNA"],"fields":[],"template":"日期：{{date}}\nRNA 提取记录","execution":{"eventType":"rna_extraction","inputTypes":["CELL","WELL"],"outputType":"RNA","outputMode":"one"}}"#],
    ).map_err(|error| error.to_string())?;

    for (id, title, start, end, status, record_id) in [
        (
            "task-template-thaw",
            "细胞复苏",
            "2026-08-24T08:00:00",
            "2026-08-24T09:00:00",
            "completed",
            Some("record-template-thaw"),
        ),
        (
            "task-template-passage",
            "细胞传代",
            "2026-08-25T09:00:00",
            "2026-08-25T10:00:00",
            "completed",
            Some("record-template-passage"),
        ),
        (
            "task-template-plating",
            "铺 6 孔板",
            "2026-08-26T09:00:00",
            "2026-08-26T10:00:00",
            "completed",
            Some("record-template-plating"),
        ),
        (
            "task-template-treatment",
            "siRNA 加刺激",
            "2026-08-27T09:00:00",
            "2026-08-27T10:00:00",
            "completed",
            Some("record-template-treatment"),
        ),
        (
            "task-template-imaging",
            "细胞成像",
            "2026-08-27T15:00:00",
            "2026-08-27T16:00:00",
            "planned",
            None,
        ),
        (
            "task-template-rna",
            "提取 RNA",
            "2026-08-28T09:00:00",
            "2026-08-28T10:30:00",
            "planned",
            None,
        ),
    ] {
        transaction.execute(
            "INSERT INTO tasks (id,experiment_id,title,start_time,end_time,status,record_id,created_at,updated_at) VALUES (?1,'exp-template',?2,?3,?4,?5,?6,?3,?3)",
            params![id,title,start,end,status,record_id],
        ).map_err(|error| error.to_string())?;
    }
    for (parent, child) in [
        ("task-template-thaw", "task-template-passage"),
        ("task-template-passage", "task-template-plating"),
        ("task-template-plating", "task-template-treatment"),
        ("task-template-plating", "task-template-imaging"),
        ("task-template-treatment", "task-template-rna"),
        ("task-template-imaging", "task-template-rna"),
    ] {
        transaction.execute(
            "INSERT INTO task_relations (id,experiment_id,parent_task_id,child_task_id,relation_type,created_at) VALUES (?1,'exp-template',?2,?3,'depends_on','2026-08-24')",
            params![format!("task-relation-{parent}-{child}"),parent,child],
        ).map_err(|error| error.to_string())?;
    }

    for (id, task_id, protocol_id, title, inputs, outputs, body, updated) in [
        (
            "record-template-thaw",
            "task-template-thaw",
            "pro-cell-thaw",
            "细胞复苏",
            json!([]),
            json!(["sample-template-cell01"]),
            "日期：2026-08-24\n复苏 A549 细胞。",
            "2026-08-24",
        ),
        (
            "record-template-passage",
            "task-template-passage",
            "pro-cell-passage",
            "细胞传代",
            json!(["sample-template-cell01"]),
            json!(["sample-template-cell02"]),
            "日期：2026-08-25\n培养方式：贴壁\n\n1. 吸除或倒掉瓶内旧培养液。\n2. PBS洗2～3次。\n3. 加入适量胰蛋白酶，轻摇使消化液流遍细胞表面。\n4. 放入培养箱消化2～5 min。\n5. 显微镜下观察，待胞质回缩、细胞间隙增大后立即终止消化，可拍打培养皿或培养瓶底部。\n6. 吸除或倒掉胰酶，加少许含血清培养液终止消化；反复轻柔吹打瓶壁细胞，从一边到底部另一边，确保细胞全部脱壁形成悬液。\n7. 计数，分别接种新的培养瓶。",
            "2026-08-25",
        ),
        (
            "record-template-plating",
            "task-template-plating",
            "pro-cell-plating",
            "铺 6 孔板",
            json!(["sample-template-cell02"]),
            json!(["sample-template-plate01"]),
            "日期：2026-08-26\n将 A549 细胞铺入 6 孔板。",
            "2026-08-26",
        ),
        (
            "record-template-treatment",
            "task-template-treatment",
            "pro-cell-treatment",
            "siRNA 加刺激",
            json!(["sample-template-plate01"]),
            json!([
                "sample-template-well01",
                "sample-template-well02",
                "sample-template-well03",
                "sample-template-well04",
                "sample-template-well05",
                "sample-template-well06"
            ]),
            "日期：2026-08-27\n1. si NC / 24h：A01, A02, A03\n2. si 123 / 24h：B01, B02, B03",
            "2026-08-27",
        ),
    ] {
        transaction.execute(
            "INSERT INTO records (id,task_id,experiment_id,protocol_id,protocol_snapshot_json,current_data_json,updated_at) VALUES (?1,?2,'exp-template',?3,?4,?5,?6)",
            params![id,task_id,protocol_id,json!({"templateSeed":true}).to_string(),json!({"title":title,"notes":"","inputs":inputs,"outputs":outputs,"renderedContent":body,"values":{}}).to_string(),updated],
        ).map_err(|error| error.to_string())?;
    }

    let samples = [
        (
            "sample-template-cell01",
            "EXP001-CELL01",
            "CELL",
            None,
            "A549 复苏细胞",
            "record-template-thaw",
            json!({"cell_name":"A549"}),
        ),
        (
            "sample-template-cell02",
            "EXP001-CELL02",
            "CELL",
            Some("sample-template-cell01"),
            "A549 传代细胞",
            "record-template-passage",
            json!({"culture_mode":"贴壁"}),
        ),
        (
            "sample-template-plate01",
            "EXP001-PLATE01",
            "PLATE",
            Some("sample-template-cell02"),
            "A549 6孔板",
            "record-template-plating",
            json!({"plate_format":"6孔板","plate_capacity":6}),
        ),
        (
            "sample-template-well01",
            "EXP001-WELL01",
            "WELL",
            Some("sample-template-plate01"),
            "A01",
            "record-template-treatment",
            json!({"well_position":"A01","treatment_factor":"si NC","treatment_duration":"24h"}),
        ),
        (
            "sample-template-well02",
            "EXP001-WELL02",
            "WELL",
            Some("sample-template-plate01"),
            "A02",
            "record-template-treatment",
            json!({"well_position":"A02","treatment_factor":"si NC","treatment_duration":"24h"}),
        ),
        (
            "sample-template-well03",
            "EXP001-WELL03",
            "WELL",
            Some("sample-template-plate01"),
            "A03",
            "record-template-treatment",
            json!({"well_position":"A03","treatment_factor":"si NC","treatment_duration":"24h"}),
        ),
        (
            "sample-template-well04",
            "EXP001-WELL04",
            "WELL",
            Some("sample-template-plate01"),
            "B01",
            "record-template-treatment",
            json!({"well_position":"B01","treatment_factor":"si 123","treatment_duration":"24h"}),
        ),
        (
            "sample-template-well05",
            "EXP001-WELL05",
            "WELL",
            Some("sample-template-plate01"),
            "B02",
            "record-template-treatment",
            json!({"well_position":"B02","treatment_factor":"si 123","treatment_duration":"24h"}),
        ),
        (
            "sample-template-well06",
            "EXP001-WELL06",
            "WELL",
            Some("sample-template-plate01"),
            "B03",
            "record-template-treatment",
            json!({"well_position":"B03","treatment_factor":"si 123","treatment_duration":"24h"}),
        ),
    ];
    for (id, code, sample_type, parent, display_name, source_record, metadata) in samples {
        transaction.execute(
            "INSERT INTO samples (id,workspace_id,experiment_id,sample_code,sample_type,source_record_id,parent_sample_id,display_name,created_at,lineage_status,metadata_json) VALUES (?1,'local','exp-template',?2,?3,?4,?5,?6,'2026-08-24','complete',?7)",
            params![id,code,sample_type,source_record,parent,display_name,metadata.to_string()],
        ).map_err(|error| error.to_string())?;
        if let Some(parent_id) = parent {
            transaction.execute(
                "INSERT INTO sample_relations (id,parent_sample_id,child_sample_id,relation_type) VALUES (?1,?2,?3,'derived_from')",
                params![format!("sample-relation-{id}"),parent_id,id],
            ).map_err(|error| error.to_string())?;
        }
    }

    for (event_id, record_id, event_type, occurred_at, input, outputs) in [
        (
            "event-template-thaw",
            "record-template-thaw",
            "thaw",
            "2026-08-24",
            None,
            vec!["sample-template-cell01"],
        ),
        (
            "event-template-passage",
            "record-template-passage",
            "passage",
            "2026-08-25",
            Some("sample-template-cell01"),
            vec!["sample-template-cell02"],
        ),
        (
            "event-template-plating",
            "record-template-plating",
            "plating",
            "2026-08-26",
            Some("sample-template-cell02"),
            vec!["sample-template-plate01"],
        ),
        (
            "event-template-treatment",
            "record-template-treatment",
            "treatment",
            "2026-08-27",
            Some("sample-template-plate01"),
            vec![
                "sample-template-well01",
                "sample-template-well02",
                "sample-template-well03",
                "sample-template-well04",
                "sample-template-well05",
                "sample-template-well06",
            ],
        ),
    ] {
        let parameters = if event_type == "treatment" {
            json!({"groups":[{"factor":"si NC","duration":"24h","wellCount":3},{"factor":"si 123","duration":"24h","wellCount":3}]})
        } else {
            json!({})
        };
        transaction.execute(
            "INSERT INTO process_events (id,experiment_id,record_id,event_type,occurred_at,parameters_json,provenance,created_at) VALUES (?1,'exp-template',?2,?3,?4,?5,'labflow_recorded',?4)",
            params![event_id,record_id,event_type,occurred_at,parameters.to_string()],
        ).map_err(|error| error.to_string())?;
        if let Some(input_id) = input {
            transaction
                .execute(
                    "INSERT INTO event_inputs VALUES (?1,?2)",
                    params![event_id, input_id],
                )
                .map_err(|error| error.to_string())?;
            transaction
                .execute(
                    "INSERT INTO record_samples VALUES (?1,?2,'input')",
                    params![record_id, input_id],
                )
                .map_err(|error| error.to_string())?;
            if event_type == "passage" {
                transaction
                    .execute(
                        "INSERT INTO sample_usages (event_id,sample_id,usage_type,created_at) VALUES (?1,?2,'consumed',?3)",
                        params![event_id, input_id, occurred_at],
                    )
                    .map_err(|error| error.to_string())?;
            }
        }
        for output_id in outputs {
            transaction
                .execute(
                    "INSERT INTO event_outputs VALUES (?1,?2)",
                    params![event_id, output_id],
                )
                .map_err(|error| error.to_string())?;
            transaction
                .execute(
                    "INSERT INTO record_samples VALUES (?1,?2,'output')",
                    params![record_id, output_id],
                )
                .map_err(|error| error.to_string())?;
        }
    }
    transaction.commit().map_err(|error| error.to_string())
}

fn read_store(connection: &Connection) -> Result<Value, String> {
    let mut experiments = Vec::new();
    let mut statement = connection
        .prepare("SELECT id, experiment_code, title, description, color FROM experiments")
        .map_err(|e| e.to_string())?;
    let rows = statement.query_map([], |row| Ok(json!({"id": row.get::<_, String>(0)?, "code": row.get::<_, String>(1)?, "title": row.get::<_, String>(2)?, "description": row.get::<_, String>(3)?, "color": row.get::<_, String>(4)?}))).map_err(|e| e.to_string())?;
    for row in rows {
        experiments.push(row.map_err(|e| e.to_string())?)
    }
    let mut tasks = Vec::new();
    let mut statement = connection
        .prepare(
            "SELECT task.id, task.experiment_id, task.title, task.start_time, task.end_time, task.status, task.record_id,
                    coalesce((SELECT json_group_array(parent_task_id) FROM task_relations WHERE child_task_id=task.id),'[]')
             FROM tasks task",
        )
        .map_err(|e| e.to_string())?;
    let rows = statement.query_map([], |row| {
        let parents: String = row.get(7)?;
        Ok(json!({"id": row.get::<_, String>(0)?, "experimentId": row.get::<_, String>(1)?, "title": row.get::<_, String>(2)?, "start": row.get::<_, String>(3)?, "end": row.get::<_, String>(4)?, "status": row.get::<_, String>(5)?, "recordId": row.get::<_, Option<String>>(6)?, "parentTaskIds": serde_json::from_str::<Value>(&parents).unwrap_or(json!([]))}))
    }).map_err(|e| e.to_string())?;
    for row in rows {
        tasks.push(row.map_err(|e| e.to_string())?)
    }
    let mut protocols = Vec::new();
    let mut statement = connection.prepare("SELECT p.id, p.name, p.category, p.active_version, p.accent, pv.schema_json, p.description, p.origin, pv.origin FROM protocols p JOIN protocol_versions pv ON pv.protocol_id=p.id AND pv.version_number=p.active_version").map_err(|e| e.to_string())?;
    let rows = statement.query_map([], |row| { let schema: String = row.get(5)?; let spec: Value = serde_json::from_str::<Value>(&schema).unwrap_or(json!({"blocks":[]})); Ok(json!({"id":row.get::<_,String>(0)? ,"name":row.get::<_,String>(1)? ,"category":row.get::<_,String>(2)? ,"version":row.get::<_,i64>(3)? ,"accent":row.get::<_,String>(4)? ,"description":row.get::<_,String>(6)?,"origin":row.get::<_,String>(7)?,"activeVersionOrigin":row.get::<_,String>(8)?,"blocks":spec["blocks"],"fields":spec["fields"],"template":spec["template"],"templateSelector":spec["templateSelector"],"templateVariants":spec["templateVariants"],"execution":spec["execution"],"terminalAssay":spec["terminalAssay"]})) }).map_err(|e| e.to_string())?;
    for row in rows {
        protocols.push(row.map_err(|e| e.to_string())?)
    }
    let mut sample_types = Vec::new();
    let mut statement = connection.prepare("SELECT canonical_type,display_name,origin FROM sample_types WHERE archived_at IS NULL ORDER BY display_name,canonical_type").map_err(|e|e.to_string())?;
    let rows = statement.query_map([], |row| Ok(json!({"canonicalType":row.get::<_,String>(0)?,"displayName":row.get::<_,String>(1)?,"origin":row.get::<_,String>(2)?}))).map_err(|e|e.to_string())?;
    for row in rows {
        sample_types.push(row.map_err(|e| e.to_string())?);
    }
    let mut samples = Vec::new();
    let mut statement = connection.prepare("SELECT s.id, s.experiment_id, s.sample_code, s.sample_type, s.source_record_id, coalesce(s.parent_sample_id,(SELECT ei.sample_id FROM event_outputs eo JOIN event_inputs ei ON ei.event_id=eo.event_id WHERE eo.sample_id=s.id AND ei.sample_id<>s.id LIMIT 1)), s.display_name, s.metadata_json, s.lineage_status, EXISTS(SELECT 1 FROM sample_usages usage WHERE usage.sample_id=s.id AND usage.usage_type='consumed'), s.origin FROM samples s WHERE s.archived_at IS NULL").map_err(|e| e.to_string())?;
    let rows = statement.query_map([], |row| { let metadata:String=row.get(7)?; Ok(json!({"id":row.get::<_,String>(0)?,"experimentId":row.get::<_,String>(1)?,"code":row.get::<_,String>(2)?,"type":row.get::<_,String>(3)?,"source":row.get::<_,Option<String>>(4)?,"parent":row.get::<_,Option<String>>(5)?,"displayName":row.get::<_,Option<String>>(6)?,"metadata":serde_json::from_str::<Value>(&metadata).unwrap_or(json!({})),"lineageStatus":row.get::<_,String>(8)?,"consumed":row.get::<_,bool>(9)?,"origin":row.get::<_,String>(10)?})) }).map_err(|e| e.to_string())?;
    for row in rows {
        samples.push(row.map_err(|e| e.to_string())?)
    }
    let mut records = Vec::new();
    let mut statement = connection.prepare("SELECT id, task_id, experiment_id, protocol_id, current_data_json, updated_at, protocol_snapshot_json FROM records").map_err(|e| e.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    for row in rows {
        let (id, task_id, experiment_id, protocol_id, data, updated, snapshot_json) =
            row.map_err(|e| e.to_string())?;
        let current: Value = serde_json::from_str(&data).map_err(|e| e.to_string())?;
        let snapshot: Value = serde_json::from_str(&snapshot_json).unwrap_or(json!({}));
        let mut history = Vec::new();
        let mut h = connection.prepare("SELECT id, field_path, old_value_json, new_value_json, changed_at FROM record_changes WHERE record_id=?1").map_err(|e| e.to_string())?;
        let changes = h.query_map([&id], |r| Ok(json!({"id":r.get::<_,String>(0)?,"field":r.get::<_,String>(1)?,"from":serde_json::from_str::<Value>(&r.get::<_,String>(2)?).unwrap_or(json!(null)),"to":serde_json::from_str::<Value>(&r.get::<_,String>(3)?).unwrap_or(json!(null)),"at":r.get::<_,String>(4)?}))).map_err(|e| e.to_string())?;
        for change in changes {
            history.push(change.map_err(|e| e.to_string())?)
        }
        let sample_ids = |role: &str| -> Result<Vec<String>, String> {
            let mut statement = connection.prepare("SELECT sample_id FROM record_samples WHERE record_id=?1 AND role=?2 ORDER BY sample_id").map_err(|error| error.to_string())?;
            let result = statement
                .query_map(params![id, role], |row| row.get(0))
                .map_err(|error| error.to_string())?
                .map(|row| row.map_err(|error| error.to_string()))
                .collect();
            result
        };
        let inputs = sample_ids("input")?;
        let outputs = sample_ids("output")?;
        let mut results = Vec::new();
        let mut result_statement = connection
            .prepare("SELECT id,result_type,structured_data_json FROM results WHERE record_id=?1 ORDER BY created_at,id")
            .map_err(|error| error.to_string())?;
        let result_rows = result_statement
            .query_map([&id], |row| {
                let data: String = row.get(2)?;
                Ok(json!({
                    "id": row.get::<_, String>(0)?,
                    "type": row.get::<_, String>(1)?,
                    "data": serde_json::from_str::<Value>(&data).unwrap_or(json!({}))
                }))
            })
            .map_err(|error| error.to_string())?;
        for result in result_rows {
            results.push(result.map_err(|error| error.to_string())?);
        }
        let mut attachments = Vec::new();
        let mut attachment_statement = connection.prepare("SELECT id,file_name,relative_path,mime_type,size FROM attachments WHERE record_id=?1 ORDER BY created_at,id").map_err(|error|error.to_string())?;
        let attachment_rows = attachment_statement.query_map([&id], |row| Ok(json!({"id":row.get::<_,String>(0)?,"fileName":row.get::<_,String>(1)?,"relativePath":row.get::<_,String>(2)?,"mimeType":row.get::<_,Option<String>>(3)?,"size":row.get::<_,Option<i64>>(4)?}))).map_err(|error|error.to_string())?;
        for attachment in attachment_rows {
            attachments.push(attachment.map_err(|error| error.to_string())?);
        }
        records.push(json!({"id":id,"taskId":task_id,"experimentId":experiment_id,"protocolId":protocol_id,"title":current["title"],"updated":updated,"notes":current["notes"],"inputs":inputs,"outputs":outputs,"results":results,"attachments":attachments,"history":history,"renderedContent":current["renderedContent"],"analysisSections":current["analysisSections"],"values":current["values"],"protocolVersion":snapshot["version"]}));
    }
    Ok(
        json!({"experiments":experiments,"tasks":tasks,"protocols":protocols,"sampleTypes":sample_types,"samples":samples,"records":records}),
    )
}

fn value_string(value: &Value, key: &str) -> Result<String, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| format!("Missing {key}"))
}
fn value_array<'a>(value: &'a Value, key: &str) -> Result<&'a Vec<Value>, String> {
    value
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("Missing {key}"))
}

fn write_store(connection: &mut Connection, store: Value) -> Result<(), String> {
    let lineage_events: i64 = connection
        .query_row("SELECT count(*) FROM process_events", [], |row| row.get(0))
        .map_err(|e| e.to_string())?;
    if lineage_events > 0 {
        return Err("Legacy full-store save is disabled once sample lineage exists; use focused repository commands so lineage cannot be overwritten.".to_string());
    }
    let transaction = connection.transaction().map_err(|e| e.to_string())?;
    for table in [
        "record_changes",
        "record_samples",
        "sample_relations",
        "samples",
        "records",
        "task_relations",
        "protocol_versions",
        "protocols",
        "tasks",
        "experiments",
    ] {
        transaction
            .execute(&format!("DELETE FROM {table}"), [])
            .map_err(|e| e.to_string())?;
    }
    for item in value_array(&store, "experiments")? {
        transaction
            .execute(
                "INSERT INTO experiments VALUES (?1,?2,?3,?4,?5)",
                params![
                    value_string(item, "id")?,
                    value_string(item, "code")?,
                    value_string(item, "title")?,
                    value_string(item, "description")?,
                    value_string(item, "color")?
                ],
            )
            .map_err(|e| e.to_string())?;
    }
    for item in value_array(&store, "protocols")? {
        let blocks = item.get("blocks").cloned().unwrap_or(json!([]));
        transaction
            .execute(
                "INSERT INTO protocols (id,name,category,active_version,accent,description,origin) VALUES (?1,?2,?3,?4,?5,?6,?7)",
                params![
                    value_string(item, "id")?,
                    value_string(item, "name")?,
                    value_string(item, "category")?,
                    item.get("version").and_then(Value::as_i64).unwrap_or(1),
                    value_string(item, "accent")?,
                    item.get("description").and_then(Value::as_str).unwrap_or(""),
                    item.get("origin").and_then(Value::as_str).unwrap_or("builtin")
                ],
            )
            .map_err(|e| e.to_string())?;
        transaction
            .execute(
                "INSERT INTO protocol_versions (protocol_id,version_number,schema_json,origin,created_at) VALUES (?1,?2,?3,?4,datetime('now'))",
                params![
                    value_string(item, "id")?,
                    item.get("version").and_then(Value::as_i64).unwrap_or(1),
                    json!({"blocks":blocks}).to_string(),
                    item.get("activeVersionOrigin").and_then(Value::as_str).unwrap_or("builtin")
                ],
            )
            .map_err(|e| e.to_string())?;
    }
    for item in value_array(&store, "tasks")? {
        transaction.execute("INSERT INTO tasks (id,experiment_id,title,start_time,end_time,status,record_id,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?4,?4)",params![value_string(item,"id")?,value_string(item,"experimentId")?,value_string(item,"title")?,value_string(item,"start")?,value_string(item,"end")?,value_string(item,"status")?,item.get("recordId").and_then(Value::as_str)]).map_err(|e|e.to_string())?;
    }
    for item in value_array(&store, "records")? {
        let protocol = value_array(&store, "protocols")?
            .iter()
            .find(|p| p.get("id") == item.get("protocolId"))
            .cloned()
            .unwrap_or(json!({}));
        let data = json!({"title":item["title"],"notes":item["notes"],"inputs":item["inputs"],"outputs":item["outputs"]});
        transaction
            .execute(
                "INSERT INTO records VALUES (?1,?2,?3,?4,?5,?6,?7)",
                params![
                    value_string(item, "id")?,
                    value_string(item, "taskId")?,
                    value_string(item, "experimentId")?,
                    value_string(item, "protocolId")?,
                    protocol.to_string(),
                    data.to_string(),
                    value_string(item, "updated")?
                ],
            )
            .map_err(|e| e.to_string())?;
        for history in value_array(item, "history")? {
            transaction
                .execute(
                    "INSERT INTO record_changes VALUES (?1,?2,?3,?4,?5,'local_user',?6)",
                    params![
                        value_string(history, "id")?,
                        value_string(item, "id")?,
                        value_string(history, "field")?,
                        history["from"].to_string(),
                        history["to"].to_string(),
                        value_string(history, "at")?
                    ],
                )
                .map_err(|e| e.to_string())?;
        }
    }
    for item in value_array(&store, "tasks")? {
        let child_id = value_string(item, "id")?;
        let experiment_id = value_string(item, "experimentId")?;
        let parent_ids = item
            .get("parentTaskIds")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|value| value.as_str().map(str::to_owned))
            .collect::<Vec<_>>();
        task_graph::replace_parents(
            &transaction,
            &child_id,
            &experiment_id,
            &parent_ids,
            "legacy-store-import",
        )?;
    }
    for item in value_array(&store, "samples")? {
        let source = item.get("source").and_then(Value::as_str);
        let experiment_id = item
            .get("experimentId")
            .and_then(Value::as_str)
            .or_else(|| {
                source.and_then(|record_id| {
                    value_array(&store, "records")
                        .ok()?
                        .iter()
                        .find(|record| record.get("id").and_then(Value::as_str) == Some(record_id))
                        .and_then(|record| record.get("experimentId"))
                        .and_then(Value::as_str)
                })
            })
            .or_else(|| {
                value_array(&store, "experiments")
                    .ok()?
                    .first()?
                    .get("id")?
                    .as_str()
            })
            .ok_or("Sample is missing Experiment")?;
        transaction
            .execute(
                "INSERT INTO samples VALUES (?1,'local',?2,?3,?4,?5,?6)",
                params![
                    value_string(item, "id")?,
                    experiment_id,
                    value_string(item, "code")?,
                    value_string(item, "type")?.to_uppercase(),
                    source,
                    item.get("parent").and_then(Value::as_str)
                ],
            )
            .map_err(|e| e.to_string())?;
        if let Some(parent) = item.get("parent").and_then(Value::as_str) {
            transaction
                .execute(
                    "INSERT INTO sample_relations VALUES (?1,?2,?3,'derived_from')",
                    params![
                        format!("rel-{parent}-{}", value_string(item, "id")?),
                        parent,
                        value_string(item, "id")?
                    ],
                )
                .map_err(|e| e.to_string())?;
        }
    }
    for record in value_array(&store, "records")? {
        for input in value_array(record, "inputs")? {
            transaction
                .execute(
                    "INSERT INTO record_samples VALUES (?1,?2,'input')",
                    params![value_string(record, "id")?, input.as_str()],
                )
                .map_err(|e| e.to_string())?;
        }
        for output in value_array(record, "outputs")? {
            transaction
                .execute(
                    "INSERT INTO record_samples VALUES (?1,?2,'output')",
                    params![value_string(record, "id")?, output.as_str()],
                )
                .map_err(|e| e.to_string())?;
        }
    }
    transaction.commit().map_err(|e| e.to_string())
}

#[tauri::command]
fn get_store(state: State<DatabaseState>) -> Result<Value, String> {
    let conn = state
        .0
        .lock()
        .map_err(|_| "Database lock poisoned".to_string())?;
    read_store(&conn)
}

#[tauri::command]
fn save_store(state: State<DatabaseState>, store: Value) -> Result<(), String> {
    let mut conn = state
        .0
        .lock()
        .map_err(|_| "Database lock poisoned".to_string())?;
    write_store(&mut conn, store)
}

fn validate_task(title: &str, start: &str, end: &str) -> Result<(), String> {
    if title.trim().is_empty() {
        return Err("Task name is required".into());
    }
    if end <= start {
        return Err("End time must be after start time".into());
    }
    Ok(())
}

#[tauri::command]
fn save_task(
    state: State<DatabaseState>,
    task: Value,
    new_experiment_name: Option<String>,
    parent_task_ids: Vec<String>,
) -> Result<Value, String> {
    let mut conn = state
        .0
        .lock()
        .map_err(|_| "Database lock poisoned".to_string())?;
    let title = value_string(&task, "title")?.trim().to_owned();
    let start = value_string(&task, "start")?;
    let end = value_string(&task, "end")?;
    validate_task(&title, &start, &end)?;
    let now = value_string(&task, "updatedAt")?;
    let id = value_string(&task, "id")?;
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    let experiment_id = if let Some(name) = new_experiment_name
        .map(|x| x.trim().to_owned())
        .filter(|x| !x.is_empty())
    {
        let eid = value_string(&task, "newExperimentId")?;
        let code = value_string(&task, "newExperimentCode")?;
        tx.execute("INSERT INTO experiments (id,experiment_code,title,description,color) VALUES (?1,?2,?3,'','#6957e8')",params![eid,code,name]).map_err(|e|e.to_string())?;
        eid
    } else {
        value_string(&task, "experimentId")?
    };
    let existing_experiment: Option<String> = tx
        .query_row(
            "SELECT experiment_id FROM tasks WHERE id=?1",
            [&id],
            |row| row.get(0),
        )
        .ok();
    if existing_experiment.as_deref().is_some_and(|existing| existing != experiment_id)
        && tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM task_relations WHERE parent_task_id=?1 OR child_task_id=?1)",
                [&id],
                |row| row.get::<_, bool>(0),
            )
            .map_err(|error| error.to_string())?
    {
        return Err("Remove this Task's dependencies before moving it to another Experiment".into());
    }
    tx.execute("INSERT INTO tasks (id,experiment_id,title,start_time,end_time,status,record_id,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,'planned',NULL,?6,?6) ON CONFLICT(id) DO UPDATE SET experiment_id=excluded.experiment_id,title=excluded.title,start_time=excluded.start_time,end_time=excluded.end_time,updated_at=excluded.updated_at",params![id,experiment_id,title,start,end,now]).map_err(|e|e.to_string())?;
    task_graph::replace_parents(&tx, &id, &experiment_id, &parent_task_ids, &now)?;
    tx.commit().map_err(|e| e.to_string())?;
    let (status, record_id): (String, Option<String>) = conn
        .query_row(
            "SELECT status,record_id FROM tasks WHERE id=?1",
            [&id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|error| error.to_string())?;
    Ok(
        json!({"id":id,"experimentId":experiment_id,"title":title,"start":start,"end":end,"status":status,"recordId":record_id,"parentTaskIds":parent_task_ids}),
    )
}

#[tauri::command]
fn delete_task(state: State<DatabaseState>, id: String) -> Result<(), String> {
    let mut conn = state
        .0
        .lock()
        .map_err(|_| "Database lock poisoned".to_string())?;
    let record: Option<String> = conn
        .query_row("SELECT record_id FROM tasks WHERE id=?1", [&id], |r| {
            r.get(0)
        })
        .map_err(|e| e.to_string())?;
    if record.is_some() {
        return Err("This task already has an experimental record and cannot be deleted.".into());
    }
    if task_graph::has_children(&conn, &id)? {
        return Err("This task is required by a downstream Task and cannot be deleted.".into());
    }
    let transaction = conn.transaction().map_err(|error| error.to_string())?;
    transaction
        .execute("DELETE FROM task_relations WHERE child_task_id=?1", [&id])
        .map_err(|error| error.to_string())?;
    if transaction
        .execute("DELETE FROM tasks WHERE id=?1", [id])
        .map_err(|e| e.to_string())?
        == 0
    {
        return Err("Task not found".into());
    }
    transaction.commit().map_err(|error| error.to_string())
}

fn delete_record_from_db(
    connection: &mut Connection,
    files_dir: &Path,
    id: &str,
) -> Result<(), String> {
    let exported: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM export_manifests manifest, json_each(manifest.record_ids_json) item WHERE item.value=?1)",
            [id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if exported {
        return Err("This Record is included in an export manifest and cannot be deleted.".into());
    }
    let has_downstream: bool = connection
        .query_row(
            "SELECT EXISTS(
               SELECT 1 FROM samples output
               WHERE output.source_record_id=?1 AND (
                 EXISTS(SELECT 1 FROM record_samples link WHERE link.sample_id=output.id AND link.record_id<>?1)
                 OR EXISTS(SELECT 1 FROM event_inputs input JOIN process_events event ON event.id=input.event_id WHERE input.sample_id=output.id AND (event.record_id IS NULL OR event.record_id<>?1))
                 OR EXISTS(SELECT 1 FROM sample_relations relation JOIN samples child ON child.id=relation.child_sample_id WHERE relation.parent_sample_id=output.id AND (child.source_record_id IS NULL OR child.source_record_id<>?1))
                 OR EXISTS(SELECT 1 FROM assay_well_mappings mapping JOIN assay_plates plate ON plate.id=mapping.plate_id WHERE mapping.sample_id=output.id AND plate.record_id<>?1)
                 OR EXISTS(SELECT 1 FROM qpcr_plate_wells legacy WHERE legacy.source_cdna_sample_id=output.id)
               )
             )",
            [id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if has_downstream {
        return Err(
            "This Record has output Samples used by downstream data and cannot be deleted.".into(),
        );
    }
    let attachment_ids = {
        let mut statement = connection
            .prepare("SELECT id FROM attachments WHERE record_id=?1")
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map([id], |row| row.get::<_, String>(0))
            .map_err(|error| error.to_string())?
            .map(|row| row.map_err(|error| error.to_string()))
            .collect::<Result<Vec<_>, _>>()?;
        rows
    };
    let transaction = connection
        .transaction()
        .map_err(|error| error.to_string())?;
    let task_id: String = transaction
        .query_row("SELECT task_id FROM records WHERE id=?1", [id], |row| {
            row.get(0)
        })
        .map_err(|_| "Record not found".to_string())?;
    transaction
        .execute(
            "DELETE FROM qpcr_delta_delta_ct_analyses WHERE record_id=?1",
            [id],
        )
        .map_err(|error| error.to_string())?;
    transaction
        .execute(
            "DELETE FROM qpcr_delta_ct_analyses WHERE record_id=?1",
            [id],
        )
        .map_err(|error| error.to_string())?;
    transaction.execute("DELETE FROM assay_raw_measurements WHERE import_id IN (SELECT id FROM assay_raw_imports WHERE record_id=?1)", [id]).map_err(|error|error.to_string())?;
    transaction
        .execute("DELETE FROM assay_raw_imports WHERE record_id=?1", [id])
        .map_err(|error| error.to_string())?;
    transaction.execute("DELETE FROM assay_well_mappings WHERE plate_id IN (SELECT id FROM assay_plates WHERE record_id=?1)", [id]).map_err(|error|error.to_string())?;
    transaction
        .execute("DELETE FROM assay_plates WHERE record_id=?1", [id])
        .map_err(|error| error.to_string())?;
    transaction
        .execute("DELETE FROM assay_items WHERE record_id=?1", [id])
        .map_err(|error| error.to_string())?;
    transaction.execute("DELETE FROM sample_usages WHERE event_id IN (SELECT id FROM process_events WHERE record_id=?1)", [id]).map_err(|error|error.to_string())?;
    transaction.execute("DELETE FROM event_inputs WHERE event_id IN (SELECT id FROM process_events WHERE record_id=?1)", [id]).map_err(|error|error.to_string())?;
    transaction.execute("DELETE FROM event_outputs WHERE event_id IN (SELECT id FROM process_events WHERE record_id=?1)", [id]).map_err(|error|error.to_string())?;
    transaction
        .execute("DELETE FROM process_events WHERE record_id=?1", [id])
        .map_err(|error| error.to_string())?;
    transaction.execute("DELETE FROM sample_aliases WHERE sample_id IN (SELECT id FROM samples WHERE source_record_id=?1)", [id]).map_err(|error|error.to_string())?;
    transaction.execute("DELETE FROM sample_locations WHERE sample_id IN (SELECT id FROM samples WHERE source_record_id=?1)", [id]).map_err(|error|error.to_string())?;
    transaction.execute("DELETE FROM sample_relations WHERE parent_sample_id IN (SELECT id FROM samples WHERE source_record_id=?1) OR child_sample_id IN (SELECT id FROM samples WHERE source_record_id=?1)", [id]).map_err(|error|error.to_string())?;
    transaction.execute("DELETE FROM record_samples WHERE record_id=?1 OR sample_id IN (SELECT id FROM samples WHERE source_record_id=?1)", [id]).map_err(|error|error.to_string())?;
    transaction
        .execute("DELETE FROM samples WHERE source_record_id=?1", [id])
        .map_err(|error| error.to_string())?;
    transaction
        .execute("DELETE FROM results WHERE record_id=?1", [id])
        .map_err(|error| error.to_string())?;
    transaction
        .execute("DELETE FROM attachments WHERE record_id=?1", [id])
        .map_err(|error| error.to_string())?;
    transaction
        .execute("DELETE FROM record_changes WHERE record_id=?1", [id])
        .map_err(|error| error.to_string())?;
    transaction.execute("UPDATE tasks SET record_id=NULL,status='planned',updated_at=datetime('now') WHERE id=?1 AND record_id=?2", params![task_id,id]).map_err(|error|error.to_string())?;
    transaction
        .execute("DELETE FROM records WHERE id=?1", [id])
        .map_err(|error| error.to_string())?;
    transaction.commit().map_err(|error| error.to_string())?;
    for attachment_id in attachment_ids {
        let directory = files_dir.join(attachment_id);
        if directory.exists() {
            fs::remove_dir_all(directory).map_err(|error| {
                format!(
                    "Record was deleted, but an attachment directory could not be removed: {error}"
                )
            })?;
        }
    }
    Ok(())
}

#[tauri::command]
fn delete_record(app: AppHandle, state: State<DatabaseState>, id: String) -> Result<(), String> {
    let mut connection = state
        .0
        .lock()
        .map_err(|_| "Database lock poisoned".to_string())?;
    delete_record_from_db(&mut connection, &attachments_dir(&app)?, &id)
}

fn update_record_body_in_db(
    connection: &mut Connection,
    id: &str,
    rendered_content: &str,
    change_id: &str,
    changed_at: &str,
) -> Result<(), String> {
    if rendered_content.trim().is_empty() {
        return Err("Record body cannot be empty.".into());
    }
    let transaction = connection
        .transaction()
        .map_err(|error| error.to_string())?;
    let current_json: String = transaction
        .query_row(
            "SELECT current_data_json FROM records WHERE id=?1",
            [id],
            |row| row.get(0),
        )
        .map_err(|_| "Record not found".to_string())?;
    let mut current: Value =
        serde_json::from_str(&current_json).map_err(|error| error.to_string())?;
    if !current.is_object() {
        return Err("Record data is invalid.".into());
    }
    let old_content = current
        .get("renderedContent")
        .cloned()
        .unwrap_or(Value::Null);
    let new_content = json!(rendered_content);
    if old_content == new_content {
        return Ok(());
    }
    current["renderedContent"] = new_content.clone();
    transaction
        .execute(
            "UPDATE records SET current_data_json=?2,updated_at=?3 WHERE id=?1",
            params![id, current.to_string(), changed_at],
        )
        .map_err(|error| error.to_string())?;
    transaction
        .execute(
            "INSERT INTO record_changes (id,record_id,field_path,old_value_json,new_value_json,actor_id,changed_at) VALUES (?1,?2,'renderedContent',?3,?4,'local_user',?5)",
            params![change_id, id, old_content.to_string(), new_content.to_string(), changed_at],
        )
        .map_err(|error| error.to_string())?;
    transaction.commit().map_err(|error| error.to_string())
}

#[tauri::command]
fn update_record_body(
    state: State<DatabaseState>,
    id: String,
    rendered_content: String,
    change_id: String,
    changed_at: String,
) -> Result<(), String> {
    let mut connection = state
        .0
        .lock()
        .map_err(|_| "Database lock poisoned".to_string())?;
    update_record_body_in_db(
        &mut connection,
        &id,
        &rendered_content,
        &change_id,
        &changed_at,
    )
}

#[tauri::command]
fn update_task_status(
    state: State<DatabaseState>,
    id: String,
    status: String,
) -> Result<Value, String> {
    if !matches!(status.as_str(), "planned" | "in_progress" | "completed") {
        return Err("Invalid task status".into());
    }
    let conn = state
        .0
        .lock()
        .map_err(|_| "Database lock poisoned".to_string())?;
    if conn
        .execute(
            "UPDATE tasks SET status=?1, updated_at=datetime('now') WHERE id=?2",
            params![status, id],
        )
        .map_err(|e| e.to_string())?
        == 0
    {
        return Err("Task not found".into());
    }
    let task = conn.query_row("SELECT id, experiment_id, title, start_time, end_time, status, record_id FROM tasks WHERE id=?1", [&id], |row| Ok(json!({"id": row.get::<_, String>(0)?, "experimentId": row.get::<_, String>(1)?, "title": row.get::<_, String>(2)?, "start": row.get::<_, String>(3)?, "end": row.get::<_, String>(4)?, "status": row.get::<_, String>(5)?, "recordId": row.get::<_, Option<String>>(6)?}))).map_err(|e| e.to_string())?;
    Ok(task)
}

fn canonical_protocol_sample_type(value: &str) -> Result<String, String> {
    let canonical = value.trim().to_uppercase();
    if canonical.is_empty()
        || canonical.len() > 32
        || !canonical.chars().enumerate().all(|(index, character)| {
            character.is_ascii_uppercase()
                || character.is_ascii_digit() && index > 0
                || character == '_' && index > 0
        })
    {
        return Err("Sample type must use 1–32 letters, numbers, or underscores".into());
    }
    Ok(canonical)
}

fn validate_protocol_template(template: &str, spec: &Value) -> Result<(), String> {
    if template.trim().is_empty() {
        return Err("Record template cannot be empty".into());
    }
    let mut allowed = vec![
        "date".to_string(),
        "input_sample_summary".to_string(),
        "output_sample_summary".to_string(),
        "plate_layout_summary".to_string(),
    ];
    for field in spec
        .get("fields")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if let Some(key) = field.get("key").and_then(Value::as_str) {
            allowed.push(key.to_owned());
        }
    }
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        let after = &rest[start + 2..];
        let end = after
            .find("}}")
            .ok_or("Record template contains an unclosed placeholder")?;
        let key = after[..end].trim();
        if !allowed.iter().any(|candidate| candidate == key) {
            return Err(format!("Unknown Record template placeholder: {key}"));
        }
        rest = &after[end + 2..];
    }
    Ok(())
}

fn register_sample_type(
    tx: &rusqlite::Transaction<'_>,
    canonical_type: &str,
    display_name: &str,
    created_at: &str,
) -> Result<(), String> {
    tx.execute(
        "INSERT OR IGNORE INTO sample_types (canonical_type,display_name,origin,created_at) VALUES (?1,?2,'user',?3)",
        params![canonical_type, display_name.trim(), created_at],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

#[tauri::command]
fn save_user_protocol(state: State<DatabaseState>, request: Value) -> Result<Value, String> {
    let mut conn = state
        .0
        .lock()
        .map_err(|_| "Database lock poisoned".to_string())?;
    save_user_protocol_to_db(&mut conn, request)
}

fn save_user_protocol_to_db(conn: &mut Connection, request: Value) -> Result<Value, String> {
    let id = value_string(&request, "id")?;
    let name = value_string(&request, "name")?;
    let description = value_string(&request, "description")?;
    let input_type = canonical_protocol_sample_type(&value_string(&request, "inputType")?)?;
    let input_display = request
        .get("inputTypeDisplayName")
        .and_then(Value::as_str)
        .unwrap_or(&input_type)
        .trim();
    let output_behavior = value_string(&request, "outputBehavior")?;
    let output_mode = match output_behavior.as_str() {
        "same_sample" => "same_sample",
        "derived_one" => "per_input",
        "derived_multiple" => "per_input_count",
        "measurement_only" => "none",
        _ => return Err("Unsupported Sample output behavior".into()),
    };
    let consumption_policy = match value_string(&request, "consumptionPolicy")?.as_str() {
        "retain" => "non_destructive",
        "consume" => "consume",
        _ => return Err("Unsupported input Sample policy".into()),
    };
    if output_mode == "same_sample" && consumption_policy == "consume" {
        return Err("A consumed Sample cannot continue as the output".into());
    }
    let output_type = if matches!(output_mode, "per_input" | "per_input_count") {
        Some(canonical_protocol_sample_type(&value_string(
            &request,
            "outputType",
        )?)?)
    } else {
        None
    };
    let template = value_string(&request, "template")?;
    let created_at = value_string(&request, "createdAt")?;
    let fields = if output_mode == "per_input_count" {
        json!([{"key":"output_count","label":"每个输入产生数量","kind":"number","required":true,"defaultValue":"2"}])
    } else {
        json!([])
    };
    let mut execution = json!({
        "engine":"sample_flow_v1",
        "eventType":format!("custom:{id}"),
        "inputSource":"experiment_samples",
        "inputCardinality":"many",
        "inputTypes":[input_type],
        "outputMode":output_mode,
        "consumptionPolicy":consumption_policy,
        "metadataPolicy":"inherit_parent"
    });
    if let Some(output_type) = &output_type {
        execution["outputType"] = json!(output_type);
    }
    let spec = json!({
        "schemaVersion":1,
        "userDefined":true,
        "blocks":["选择输入 Sample", "按模板记录实验过程", match output_mode { "same_sample" => "原 Sample 继续", "per_input" => "每个输入产生一个新 Sample", "per_input_count" => "每个输入产生多个新 Sample", _ => "仅记录检测，不产生 Sample" }],
        "fields":fields,
        "template":template,
        "execution":execution
    });
    validate_protocol_template(spec["template"].as_str().unwrap_or(""), &spec)?;
    let tx = conn.transaction().map_err(|error| error.to_string())?;
    let exists: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM protocols WHERE id=?1)",
            [&id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if exists {
        return Err("Protocol id already exists".into());
    }
    register_sample_type(&tx, &input_type, input_display, &created_at)?;
    if let Some(output_type) = &output_type {
        let output_display = request
            .get("outputTypeDisplayName")
            .and_then(Value::as_str)
            .unwrap_or(output_type);
        register_sample_type(&tx, output_type, output_display, &created_at)?;
    }
    tx.execute(
        "INSERT INTO protocols (id,name,category,active_version,accent,description,origin) VALUES (?1,?2,?3,1,?4,?5,'user')",
        params![id, name.trim(), request.get("category").and_then(Value::as_str).unwrap_or("自定义"), request.get("accent").and_then(Value::as_str).unwrap_or("#6957e8"), description.trim()],
    ).map_err(|error|error.to_string())?;
    tx.execute(
        "INSERT INTO protocol_versions (protocol_id,version_number,schema_json,origin,created_at) VALUES (?1,1,?2,'user',?3)",
        params![id, spec.to_string(), created_at],
    ).map_err(|error|error.to_string())?;
    tx.commit().map_err(|error| error.to_string())?;
    Ok(json!({"id":id,"version":1}))
}

#[tauri::command]
fn save_protocol_template_version(
    state: State<DatabaseState>,
    request: Value,
) -> Result<Value, String> {
    let mut conn = state
        .0
        .lock()
        .map_err(|_| "Database lock poisoned".to_string())?;
    save_protocol_template_version_to_db(&mut conn, request)
}

fn save_protocol_template_version_to_db(
    conn: &mut Connection,
    request: Value,
) -> Result<Value, String> {
    let protocol_id = value_string(&request, "protocolId")?;
    let created_at = value_string(&request, "createdAt")?;
    let tx = conn.transaction().map_err(|error| error.to_string())?;
    let (active_version, schema): (i64, String) = tx.query_row(
        "SELECT p.active_version,pv.schema_json FROM protocols p JOIN protocol_versions pv ON pv.protocol_id=p.id AND pv.version_number=p.active_version WHERE p.id=?1",
        [&protocol_id],
        |row| Ok((row.get(0)?,row.get(1)?)),
    ).map_err(|_|"Protocol not found".to_string())?;
    let mut spec: Value =
        serde_json::from_str(&schema).map_err(|_| "Protocol schema is invalid".to_string())?;
    if spec
        .get("templateVariants")
        .and_then(Value::as_object)
        .is_some()
    {
        let variants = request
            .get("templateVariants")
            .and_then(Value::as_object)
            .ok_or("This Protocol requires all template variants")?;
        let existing = spec
            .get("templateVariants")
            .and_then(Value::as_object)
            .unwrap();
        for key in existing.keys() {
            let value = variants
                .get(key)
                .and_then(Value::as_str)
                .ok_or_else(|| format!("Missing template variant: {key}"))?;
            validate_protocol_template(value, &spec)?;
        }
        spec["templateVariants"] = Value::Object(variants.clone());
    } else {
        let template = value_string(&request, "template")?;
        validate_protocol_template(&template, &spec)?;
        spec["template"] = json!(template);
    }
    let next_version: i64 = tx
        .query_row(
            "SELECT coalesce(max(version_number),0)+1 FROM protocol_versions WHERE protocol_id=?1",
            [&protocol_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    tx.execute("INSERT INTO protocol_versions (protocol_id,version_number,schema_json,origin,created_at) VALUES (?1,?2,?3,'user',?4)",params![protocol_id,next_version,spec.to_string(),created_at]).map_err(|error|error.to_string())?;
    tx.execute(
        "UPDATE protocols SET active_version=?2 WHERE id=?1",
        params![protocol_id, next_version],
    )
    .map_err(|error| error.to_string())?;
    tx.commit().map_err(|error| error.to_string())?;
    Ok(json!({"id":protocol_id,"previousVersion":active_version,"version":next_version}))
}

#[tauri::command]
fn start_task_record(
    state: State<DatabaseState>,
    task_id: String,
    protocol_id: String,
    record_id: String,
    values: Value,
    input_sample_ids: Vec<String>,
    external_inputs: Vec<Value>,
) -> Result<Value, String> {
    let mut conn = state
        .0
        .lock()
        .map_err(|_| "Database lock poisoned".to_string())?;
    protocol_execution::execute_with_external(
        &mut conn,
        &task_id,
        &protocol_id,
        &record_id,
        values,
        input_sample_ids,
        external_inputs,
    )
    .map(|result| result.task)
}

#[tauri::command]
fn get_assay_workspace(state: State<DatabaseState>, record_id: String) -> Result<Value, String> {
    let conn = state
        .0
        .lock()
        .map_err(|_| "Database lock poisoned".to_string())?;
    terminal_assay::workspace(&conn, &record_id)
}

#[tauri::command]
fn create_assay_plate(state: State<DatabaseState>, request: Value) -> Result<(), String> {
    let conn = state
        .0
        .lock()
        .map_err(|_| "Database lock poisoned".to_string())?;
    terminal_assay::create_plate(&conn, &request)
}

#[tauri::command]
fn delete_empty_assay_plate(state: State<DatabaseState>, plate_id: String) -> Result<(), String> {
    let conn = state
        .0
        .lock()
        .map_err(|_| "Database lock poisoned".to_string())?;
    terminal_assay::delete_empty_plate(&conn, &plate_id)
}

#[tauri::command]
fn replace_assay_plate_mappings(
    state: State<DatabaseState>,
    plate_id: String,
    mappings: Vec<Value>,
    changed_at: String,
) -> Result<(), String> {
    let mut conn = state
        .0
        .lock()
        .map_err(|_| "Database lock poisoned".to_string())?;
    terminal_assay::replace_mappings(&mut conn, &plate_id, &mappings, &changed_at)
}

#[tauri::command]
fn upload_assay_raw_file(
    app: AppHandle,
    state: State<DatabaseState>,
    request: Value,
    bytes: Vec<u8>,
) -> Result<Value, String> {
    let mut conn = state
        .0
        .lock()
        .map_err(|_| "Database lock poisoned".to_string())?;
    terminal_assay::upload_raw(&mut conn, &attachments_dir(&app)?, &request, &bytes)
}

#[tauri::command]
fn create_qpcr_delta_ct_analysis(
    state: State<DatabaseState>,
    request: Value,
) -> Result<Value, String> {
    let mut conn = state
        .0
        .lock()
        .map_err(|_| "Database lock poisoned".to_string())?;
    terminal_assay::create_delta_ct_analysis(&mut conn, &request)
}

#[tauri::command]
fn create_qpcr_delta_delta_ct_analysis(
    state: State<DatabaseState>,
    request: Value,
) -> Result<Value, String> {
    let mut conn = state
        .0
        .lock()
        .map_err(|_| "Database lock poisoned".to_string())?;
    terminal_assay::create_delta_delta_ct_analysis(&mut conn, &request)
}

/// Focused desktop repository commands.  New sample/lineage workflows use
/// these commands rather than the legacy full-store writer above, so an edit
/// cannot erase unrelated local data.
#[tauri::command]
fn create_process_event(
    state: State<DatabaseState>,
    event: Value,
    input_ids: Vec<String>,
    outputs: Vec<Value>,
) -> Result<(), String> {
    let mut conn = state
        .0
        .lock()
        .map_err(|_| "Database lock poisoned".to_string())?;
    let id = value_string(&event, "id")?;
    let experiment_id = value_string(&event, "experimentId")?;
    let occurred_at = value_string(&event, "occurredAt")?;
    let event_type = value_string(&event, "eventType")?;
    let provenance = event
        .get("provenance")
        .and_then(Value::as_str)
        .unwrap_or("labflow_recorded");
    let parameters = event.get("parameters").cloned().unwrap_or(json!({}));
    let record_id = event.get("recordId").and_then(Value::as_str);
    let output_values: Result<Vec<_>, String> = outputs
        .iter()
        .map(|sample| {
            Ok(lineage::NewSample {
                id: value_string(sample, "id")?,
                experiment_id: experiment_id.clone(),
                code: value_string(sample, "code")?,
                display_name: sample
                    .get("displayName")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned(),
                sample_type: value_string(sample, "sampleType")?,
                lineage_status: sample
                    .get("lineageStatus")
                    .and_then(Value::as_str)
                    .unwrap_or("complete")
                    .to_owned(),
                metadata: sample.get("metadata").cloned().unwrap_or(json!({})),
            })
        })
        .collect();
    let output_values = output_values?;
    let input_refs: Vec<&str> = input_ids.iter().map(String::as_str).collect();
    lineage::create_event(
        &mut conn,
        lineage::Event {
            id: &id,
            experiment_id: &experiment_id,
            record_id,
            event_type: &event_type,
            occurred_at: &occurred_at,
            parameters,
            provenance,
        },
        &input_refs,
        &output_values,
    )
}

#[tauri::command]
fn apply_treatment_event(
    state: State<DatabaseState>,
    event: Value,
    sample_ids: Vec<String>,
) -> Result<(), String> {
    let mut conn = state
        .0
        .lock()
        .map_err(|_| "Database lock poisoned".to_string())?;
    let id = value_string(&event, "id")?;
    let experiment_id = value_string(&event, "experimentId")?;
    let occurred_at = value_string(&event, "occurredAt")?;
    let parameters = event.get("parameters").cloned().unwrap_or(json!({}));
    let provenance = event
        .get("provenance")
        .and_then(Value::as_str)
        .unwrap_or("labflow_recorded");
    let record_id = event.get("recordId").and_then(Value::as_str);
    let refs: Vec<&str> = sample_ids.iter().map(String::as_str).collect();
    lineage::apply_treatment(
        &mut conn,
        lineage::Event {
            id: &id,
            experiment_id: &experiment_id,
            record_id,
            event_type: "treatment",
            occurred_at: &occurred_at,
            parameters,
            provenance,
        },
        &refs,
    )
}

#[tauri::command]
fn create_treatment_definition(
    state: State<DatabaseState>,
    treatment: Value,
) -> Result<(), String> {
    let conn = state
        .0
        .lock()
        .map_err(|_| "Database lock poisoned".to_string())?;
    let now = value_string(&treatment, "createdAt")?;
    conn.execute("INSERT INTO treatment_definitions (id,experiment_id,short_code,name,parameters_json,created_at,archived_at) VALUES (?1,?2,?3,?4,?5,?6,NULL)", params![value_string(&treatment,"id")?,value_string(&treatment,"experimentId")?,value_string(&treatment,"shortCode")?,value_string(&treatment,"name")?,treatment.get("parameters").cloned().unwrap_or(json!({})).to_string(),now]).map_err(|e|e.to_string())?;
    Ok(())
}

#[tauri::command]
fn update_treatment_definition(
    state: State<DatabaseState>,
    treatment: Value,
) -> Result<(), String> {
    let conn = state
        .0
        .lock()
        .map_err(|_| "Database lock poisoned".to_string())?;
    let id = value_string(&treatment, "id")?;
    let changed=conn.execute("UPDATE treatment_definitions SET short_code=?2,name=?3,parameters_json=?4 WHERE id=?1 AND archived_at IS NULL",params![id,value_string(&treatment,"shortCode")?,value_string(&treatment,"name")?,treatment.get("parameters").cloned().unwrap_or(json!({})).to_string()]).map_err(|e|e.to_string())?;
    if changed == 0 {
        return Err("Active treatment definition not found".into());
    }
    let changed_at = treatment
        .get("updatedAt")
        .and_then(Value::as_str)
        .unwrap_or("local-update")
        .to_owned();
    lineage::audit(
        &conn,
        &format!("change-{id}-{changed_at}"),
        "treatment_definition",
        &id,
        "$",
        json!(null),
        treatment,
        &changed_at,
    )
}

#[tauri::command]
fn archive_treatment_definition(
    state: State<DatabaseState>,
    id: String,
    archived_at: String,
) -> Result<(), String> {
    let conn = state
        .0
        .lock()
        .map_err(|_| "Database lock poisoned".to_string())?;
    let changed = conn
        .execute(
            "UPDATE treatment_definitions SET archived_at=?2 WHERE id=?1",
            params![id, archived_at],
        )
        .map_err(|e| e.to_string())?;
    if changed == 0 {
        return Err("Treatment definition not found".to_string());
    }
    Ok(())
}

#[tauri::command]
fn sample_detail(state: State<DatabaseState>, sample_id: String) -> Result<Value, String> {
    let conn = state
        .0
        .lock()
        .map_err(|_| "Database lock poisoned".to_string())?;
    let sample=conn.query_row("SELECT id,experiment_id,sample_code,display_name,sample_type,lineage_status,metadata_json,created_at FROM samples WHERE id=?1",[&sample_id],|r| { let metadata:String=r.get(6)?; Ok(json!({"id":r.get::<_,String>(0)?,"experimentId":r.get::<_,String>(1)?,"code":r.get::<_,String>(2)?,"displayName":r.get::<_,Option<String>>(3)?,"sampleType":r.get::<_,String>(4)?,"lineageStatus":r.get::<_,Option<String>>(5)?,"metadata":serde_json::from_str::<Value>(&metadata).unwrap_or(json!({})),"createdAt":r.get::<_,Option<String>>(7)?})) }).map_err(|e|e.to_string())?;
    let mut aliases = Vec::new();
    let mut s=conn.prepare("SELECT id,alias,alias_type,created_at FROM sample_aliases WHERE sample_id=?1 ORDER BY created_at").map_err(|e|e.to_string())?;
    for row in s.query_map([&sample_id],|r|Ok(json!({"id":r.get::<_,String>(0)?,"alias":r.get::<_,String>(1)?,"aliasType":r.get::<_,String>(2)?,"createdAt":r.get::<_,String>(3)?}))).map_err(|e|e.to_string())? { aliases.push(row.map_err(|e|e.to_string())?) }
    Ok(
        json!({"sample":sample,"aliases":aliases,"upstream":lineage::upstream(&conn,&sample_id)?,"treatments":lineage::derived_treatments(&conn,&sample_id)?}),
    )
}

#[tauri::command]
fn add_sample_alias(state: State<DatabaseState>, alias: Value) -> Result<(), String> {
    let conn = state
        .0
        .lock()
        .map_err(|_| "Database lock poisoned".to_string())?;
    conn.execute(
        "INSERT INTO sample_aliases VALUES (?1,?2,?3,?4,?5)",
        params![
            value_string(&alias, "id")?,
            value_string(&alias, "sampleId")?,
            value_string(&alias, "alias")?,
            alias
                .get("aliasType")
                .and_then(Value::as_str)
                .unwrap_or("legacy"),
            value_string(&alias, "createdAt")?
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
#[tauri::command]
fn delete_sample_alias(state: State<DatabaseState>, id: String) -> Result<(), String> {
    let conn = state
        .0
        .lock()
        .map_err(|_| "Database lock poisoned".to_string())?;
    if conn
        .execute("DELETE FROM sample_aliases WHERE id=?1", [id])
        .map_err(|e| e.to_string())?
        == 0
    {
        return Err("Alias not found".into());
    };
    Ok(())
}

#[tauri::command]
fn save_experiment(
    state: State<DatabaseState>,
    experiment: Value,
    changed_at: String,
) -> Result<(), String> {
    let conn = state
        .0
        .lock()
        .map_err(|_| "Database lock poisoned".to_string())?;
    let id = value_string(&experiment, "id")?;
    let existing: Option<Value>=conn.query_row("SELECT experiment_code,title,description,color FROM experiments WHERE id=?1",[&id],|r|Ok(json!({"code":r.get::<_,String>(0)?,"title":r.get::<_,String>(1)?,"description":r.get::<_,String>(2)?,"color":r.get::<_,String>(3)?}))).ok();
    conn.execute("INSERT INTO experiments (id,experiment_code,title,description,color) VALUES (?1,?2,?3,?4,?5) ON CONFLICT(id) DO UPDATE SET experiment_code=excluded.experiment_code,title=excluded.title,description=excluded.description,color=excluded.color",params![id,value_string(&experiment,"code")?,value_string(&experiment,"title")?,experiment.get("description").and_then(Value::as_str).unwrap_or(""),experiment.get("color").and_then(Value::as_str).unwrap_or("#6957e8")]).map_err(|e|e.to_string())?;
    lineage::audit(
        &conn,
        &format!("change-{id}-{changed_at}"),
        "experiment",
        &id,
        "$",
        existing.unwrap_or(json!(null)),
        experiment,
        &changed_at,
    )
}

#[tauri::command]
fn delete_experiment(state: State<DatabaseState>, id: String) -> Result<(), String> {
    let conn = state
        .0
        .lock()
        .map_err(|_| "Database lock poisoned".to_string())?;
    let dependent:i64=conn.query_row("SELECT (SELECT count(*) FROM tasks WHERE experiment_id=?1)+(SELECT count(*) FROM samples WHERE experiment_id=?1)+(SELECT count(*) FROM process_events WHERE experiment_id=?1)",[&id],|r|r.get(0)).map_err(|e|e.to_string())?;
    if dependent > 0 {
        return Err("Cannot delete an experiment with tasks, samples, or lineage history".into());
    }
    if conn
        .execute("DELETE FROM experiments WHERE id=?1", [id])
        .map_err(|e| e.to_string())?
        == 0
    {
        return Err("Experiment not found".into());
    };
    Ok(())
}

#[tauri::command]
fn create_container(state: State<DatabaseState>, container: Value) -> Result<(), String> {
    let conn = state
        .0
        .lock()
        .map_err(|_| "Database lock poisoned".to_string())?;
    conn.execute("INSERT INTO containers (id,experiment_id,container_type,name,metadata_json,created_at) VALUES (?1,?2,?3,?4,?5,?6)",params![value_string(&container,"id")?,value_string(&container,"experimentId")?,value_string(&container,"containerType")?,value_string(&container,"name")?,container.get("metadata").cloned().unwrap_or(json!({})).to_string(),value_string(&container,"createdAt")?]).map_err(|e|e.to_string())?;
    Ok(())
}
#[tauri::command]
fn delete_container(state: State<DatabaseState>, id: String) -> Result<(), String> {
    let conn = state
        .0
        .lock()
        .map_err(|_| "Database lock poisoned".to_string())?;
    let refs: i64 = conn
        .query_row(
            "SELECT count(*) FROM sample_locations WHERE container_id=?1",
            [&id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if refs > 0 {
        return Err("Cannot delete a container with location history".into());
    };
    if conn
        .execute("DELETE FROM containers WHERE id=?1", [id])
        .map_err(|e| e.to_string())?
        == 0
    {
        return Err("Container not found".into());
    };
    Ok(())
}

#[tauri::command]
fn assign_sample_location(state: State<DatabaseState>, location: Value) -> Result<(), String> {
    let mut conn = state
        .0
        .lock()
        .map_err(|_| "Database lock poisoned".to_string())?;
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    let sample_id = value_string(&location, "sampleId")?;
    let valid_from = value_string(&location, "validFrom")?;
    tx.execute(
        "UPDATE sample_locations SET valid_until=?2 WHERE sample_id=?1 AND valid_until IS NULL",
        params![sample_id, valid_from],
    )
    .map_err(|e| e.to_string())?;
    tx.execute("INSERT INTO sample_locations (id,sample_id,container_id,position,valid_from,valid_until) VALUES (?1,?2,?3,?4,?5,NULL)",params![value_string(&location,"id")?,sample_id,value_string(&location,"containerId")?,value_string(&location,"position")?,valid_from]).map_err(|e|e.to_string())?;
    tx.commit().map_err(|e| e.to_string())
}

#[tauri::command]
fn create_qpcr_mapping(state: State<DatabaseState>, mapping: Value) -> Result<(), String> {
    let conn = state
        .0
        .lock()
        .map_err(|_| "Database lock poisoned".to_string())?;
    conn.execute("INSERT INTO qpcr_plate_wells (id,experiment_id,source_cdna_sample_id,target_name,technical_replicate_index,plate_position,created_at) VALUES (?1,?2,?3,?4,?5,?6,?7)",params![value_string(&mapping,"id")?,value_string(&mapping,"experimentId")?,value_string(&mapping,"sampleId")?,value_string(&mapping,"targetName")?,mapping.get("technicalReplicateIndex").and_then(Value::as_i64).unwrap_or(1),value_string(&mapping,"platePosition")?,value_string(&mapping,"createdAt")?]).map_err(|e|e.to_string())?;
    Ok(())
}
#[tauri::command]
fn delete_qpcr_mapping(state: State<DatabaseState>, id: String) -> Result<(), String> {
    let conn = state
        .0
        .lock()
        .map_err(|_| "Database lock poisoned".to_string())?;
    if conn
        .execute("DELETE FROM qpcr_plate_wells WHERE id=?1", [id])
        .map_err(|e| e.to_string())?
        == 0
    {
        return Err("qPCR mapping not found".into());
    };
    Ok(())
}

/// Preserve the audit chain once a material is consumed by a later event.
#[tauri::command]
fn delete_or_archive_sample(
    state: State<DatabaseState>,
    id: String,
    archived_at: String,
) -> Result<String, String> {
    let mut conn = state
        .0
        .lock()
        .map_err(|_| "Database lock poisoned".to_string())?;
    let downstream:i64=conn.query_row("SELECT count(*) FROM event_inputs ei WHERE ei.sample_id=?1 AND NOT EXISTS (SELECT 1 FROM event_outputs own WHERE own.event_id=ei.event_id AND own.sample_id=ei.sample_id)",[&id],|r|r.get(0)).map_err(|e|e.to_string())?;
    if downstream > 0 {
        if conn
            .execute(
                "UPDATE samples SET archived_at=?2 WHERE id=?1",
                params![id, archived_at],
            )
            .map_err(|e| e.to_string())?
            == 0
        {
            return Err("Sample not found".into());
        };
        return Ok("archived".into());
    }
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    let found: i64 = tx
        .query_row("SELECT count(*) FROM samples WHERE id=?1", [&id], |r| {
            r.get(0)
        })
        .map_err(|e| e.to_string())?;
    if found == 0 {
        return Err("Sample not found".into());
    };
    tx.execute("DELETE FROM sample_aliases WHERE sample_id=?1", [&id])
        .map_err(|e| e.to_string())?;
    tx.execute("DELETE FROM sample_locations WHERE sample_id=?1", [&id])
        .map_err(|e| e.to_string())?;
    tx.execute(
        "DELETE FROM qpcr_plate_wells WHERE source_cdna_sample_id=?1",
        [&id],
    )
    .map_err(|e| e.to_string())?;
    tx.execute("DELETE FROM record_samples WHERE sample_id=?1", [&id])
        .map_err(|e| e.to_string())?;
    tx.execute("DELETE FROM event_inputs WHERE sample_id=?1", [&id])
        .map_err(|e| e.to_string())?;
    tx.execute("DELETE FROM event_outputs WHERE sample_id=?1", [&id])
        .map_err(|e| e.to_string())?;
    tx.execute("DELETE FROM samples WHERE id=?1", [&id])
        .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok("deleted".into())
}

#[tauri::command]
fn delete_or_archive_process_event(
    state: State<DatabaseState>,
    id: String,
    archived_at: String,
) -> Result<String, String> {
    let mut conn = state
        .0
        .lock()
        .map_err(|_| "Database lock poisoned".to_string())?;
    let downstream:i64=conn.query_row("SELECT count(*) FROM event_outputs source JOIN event_inputs next ON next.sample_id=source.sample_id WHERE source.event_id=?1 AND next.event_id<>source.event_id",[&id],|r|r.get(0)).map_err(|e|e.to_string())?;
    if downstream > 0 {
        if conn
            .execute(
                "UPDATE process_events SET archived_at=?2 WHERE id=?1",
                params![id, archived_at],
            )
            .map_err(|e| e.to_string())?
            == 0
        {
            return Err("Process event not found".into());
        };
        return Ok("archived".into());
    }
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    let found: i64 = tx
        .query_row(
            "SELECT count(*) FROM process_events WHERE id=?1",
            [&id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if found == 0 {
        return Err("Process event not found".into());
    };
    tx.execute("DELETE FROM event_inputs WHERE event_id=?1", [&id])
        .map_err(|e| e.to_string())?;
    tx.execute("DELETE FROM event_outputs WHERE event_id=?1", [&id])
        .map_err(|e| e.to_string())?;
    tx.execute("DELETE FROM process_events WHERE id=?1", [&id])
        .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok("deleted".into())
}

#[tauri::command]
fn lineage_workspace(state: State<DatabaseState>, experiment_id: String) -> Result<Value, String> {
    let conn = state
        .0
        .lock()
        .map_err(|_| "Database lock poisoned".to_string())?;
    fn rows(conn: &Connection, sql: &str, id: &str) -> Result<Vec<Value>, String> {
        let mut statement = conn.prepare(sql).map_err(|e| e.to_string())?;
        let values = statement
            .query_map([id], |r| {
                let text: String = r.get(0)?;
                serde_json::from_str(&text).map_err(|_| rusqlite::Error::InvalidQuery)
            })
            .map_err(|e| e.to_string())?;
        values.map(|v| v.map_err(|e| e.to_string())).collect()
    }
    let samples=rows(&conn,"SELECT json_object('id',id,'code',sample_code,'displayName',coalesce(display_name,''),'sampleType',sample_type,'lineageStatus',coalesce(lineage_status,'complete'),'archivedAt',archived_at) FROM samples WHERE experiment_id=?1 ORDER BY created_at,sample_code",&experiment_id)?;
    let treatments=rows(&conn,"SELECT json_object('id',id,'shortCode',short_code,'name',name,'parameters',json(parameters_json),'archivedAt',archived_at) FROM treatment_definitions WHERE experiment_id=?1 ORDER BY created_at",&experiment_id)?;
    let containers=rows(&conn,"SELECT json_object('id',id,'containerType',container_type,'name',name,'metadata',json(metadata_json)) FROM containers WHERE experiment_id=?1 ORDER BY created_at",&experiment_id)?;
    let mappings=rows(&conn,"SELECT json_object('id',id,'sampleId',source_cdna_sample_id,'targetName',target_name,'technicalReplicateIndex',technical_replicate_index,'platePosition',plate_position) FROM qpcr_plate_wells WHERE experiment_id=?1 ORDER BY plate_position",&experiment_id)?;
    let events=rows(&conn,"SELECT json_object('id',id,'eventType',event_type,'occurredAt',occurred_at,'archivedAt',archived_at) FROM process_events WHERE experiment_id=?1 ORDER BY occurred_at",&experiment_id)?;
    Ok(
        json!({"samples":samples,"treatments":treatments,"containers":containers,"qpcrMappings":mappings,"events":events}),
    )
}

#[tauri::command]
fn user_data_location(app: AppHandle) -> Result<String, String> {
    app_data_dir(&app).map(|path| path.display().to_string())
}

#[tauri::command]
fn export_workspace_backup(
    app: AppHandle,
    state: State<DatabaseState>,
    destination: String,
    exported_at: String,
) -> Result<workspace_backup::ExportResult, String> {
    let connection = state
        .0
        .lock()
        .map_err(|_| "Database lock poisoned".to_string())?;
    workspace_backup::export_workspace(
        &connection,
        &attachments_dir(&app)?,
        Path::new(&destination),
        &exported_at,
    )
}

#[tauri::command]
fn inspect_workspace_backup(
    app: AppHandle,
    path: String,
) -> Result<workspace_backup::BackupSummary, String> {
    workspace_backup::inspect_backup(Path::new(&path), &app_data_dir(&app)?)
}

#[tauri::command]
fn restore_workspace_backup(
    app: AppHandle,
    state: State<DatabaseState>,
    path: String,
    imported_at: String,
) -> Result<workspace_backup::RestoreResult, String> {
    let mut connection = state
        .0
        .lock()
        .map_err(|_| "Database lock poisoned".to_string())?;
    workspace_backup::restore_workspace(
        &mut connection,
        &app_data_dir(&app)?,
        Path::new(&path),
        &imported_at,
        |restored| {
            apply_schema(restored)?;
            ensure_builtin_protocols(restored)
        },
    )
}

fn export_record_snapshot(connection: &Connection, record_id: &str) -> Result<Value, String> {
    let (id, task_id, task_title, task_start, experiment_code, experiment_title, protocol_name, protocol_snapshot, current_data, updated_at): (String, String, String, String, String, String, String, String, String, String) = connection
        .query_row(
            "SELECT record.id,task.id,task.title,task.start_time,experiment.experiment_code,experiment.title,protocol.name,record.protocol_snapshot_json,record.current_data_json,record.updated_at FROM records record JOIN tasks task ON task.id=record.task_id JOIN experiments experiment ON experiment.id=record.experiment_id JOIN protocols protocol ON protocol.id=record.protocol_id WHERE record.id=?1",
            [record_id],
            |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?,row.get(6)?,row.get(7)?,row.get(8)?,row.get(9)?)),
        )
        .map_err(|_| format!("Export Record not found: {record_id}"))?;
    let linked_rows = |sql: &str| -> Result<Vec<Value>, String> {
        let mut statement = connection.prepare(sql).map_err(|error| error.to_string())?;
        let rows = statement
            .query_map([record_id], |row| {
                let raw: String = row.get(0)?;
                Ok(serde_json::from_str::<Value>(&raw).unwrap_or(json!({})))
            })
            .map_err(|error| error.to_string())?
            .map(|row| row.map_err(|error| error.to_string()))
            .collect();
        rows
    };
    let samples = linked_rows("SELECT json_object('role',link.role,'id',sample.id,'code',sample.sample_code,'type',sample.sample_type,'displayName',coalesce(sample.display_name,'')) FROM record_samples link JOIN samples sample ON sample.id=link.sample_id WHERE link.record_id=?1 ORDER BY link.role,sample.sample_code")?;
    let results = linked_rows("SELECT json_object('id',id,'type',result_type,'data',json(structured_data_json)) FROM results WHERE record_id=?1 ORDER BY created_at,id")?;
    let attachments = linked_rows("SELECT json_object('id',id,'fileName',file_name,'relativePath',relative_path,'mimeType',coalesce(mime_type,''),'size',coalesce(size,0)) FROM attachments WHERE record_id=?1 ORDER BY created_at,id")?;
    Ok(json!({
        "id":id,
        "taskId":task_id,
        "taskTitle":task_title,
        "taskStart":task_start,
        "experimentCode":experiment_code,
        "experimentTitle":experiment_title,
        "protocolName":protocol_name,
        "protocolSnapshot":serde_json::from_str::<Value>(&protocol_snapshot).unwrap_or(json!({})),
        "currentData":serde_json::from_str::<Value>(&current_data).unwrap_or(json!({})),
        "updatedAt":updated_at,
        "samples":samples,
        "results":results,
        "attachments":attachments
    }))
}

fn create_export_manifest_at(
    connection: &Connection,
    files_dir: &Path,
    request: &Value,
) -> Result<Value, String> {
    let id = value_string(request, "id")?;
    if !id
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || character == '-')
    {
        return Err("Export id contains unsupported characters".into());
    }
    let date_from = value_string(request, "dateFrom")?;
    let date_to = value_string(request, "dateTo")?;
    if date_from > date_to {
        return Err("Export start date must not be after end date".into());
    }
    let created_at = value_string(request, "createdAt")?;
    let mut record_ids = value_array(request, "recordIds")?
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect::<Vec<_>>();
    record_ids.sort();
    record_ids.dedup();
    if record_ids.is_empty() {
        return Err("Select at least one Record to export".into());
    }
    let mut records = record_ids
        .iter()
        .map(|record_id| export_record_snapshot(connection, record_id))
        .collect::<Result<Vec<_>, _>>()?;
    records.sort_by(|left, right| {
        left["taskStart"]
            .as_str()
            .cmp(&right["taskStart"].as_str())
            .then_with(|| left["id"].as_str().cmp(&right["id"].as_str()))
    });
    if records.iter().any(|record| {
        let date = record["taskStart"]
            .as_str()
            .unwrap_or("")
            .split('T')
            .next()
            .unwrap_or("");
        date < date_from.as_str() || date > date_to.as_str()
    }) {
        return Err("Selected Record falls outside the export date range".into());
    }
    let payload = json!({
        "schemaVersion":1,
        "exportId":id,
        "dateFrom":date_from,
        "dateTo":date_to,
        "createdAt":created_at,
        "records":records
    });
    let payload_bytes = serde_json::to_vec(&payload).map_err(|error| error.to_string())?;
    let content_sha256 = format!("{:x}", Sha256::digest(&payload_bytes));
    let manifest = json!({"contentSha256":content_sha256,"payload":payload});
    let relative_path = format!("files/exports/{id}/manifest.json");
    let export_dir = files_dir.join("exports").join(&id);
    fs::create_dir_all(&export_dir).map_err(|error| error.to_string())?;
    let manifest_path = export_dir.join("manifest.json");
    fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    let insert = connection.execute(
        "INSERT INTO export_manifests (id,date_from,date_to,record_ids_json,content_sha256,relative_path,status,created_at) VALUES (?1,?2,?3,?4,?5,?6,'previewed',?7)",
        params![id,date_from,date_to,serde_json::to_string(&record_ids).map_err(|error|error.to_string())?,content_sha256,relative_path,created_at],
    );
    if let Err(error) = insert {
        let _ = fs::remove_file(&manifest_path);
        return Err(error.to_string());
    }
    Ok(
        json!({"id":id,"contentSha256":content_sha256,"relativePath":relative_path,"recordCount":record_ids.len()}),
    )
}

#[tauri::command]
fn create_export_manifest(
    app: AppHandle,
    state: State<DatabaseState>,
    request: Value,
) -> Result<Value, String> {
    let conn = state
        .0
        .lock()
        .map_err(|_| "Database lock poisoned".to_string())?;
    create_export_manifest_at(&conn, &attachments_dir(&app)?, &request)
}

#[tauri::command]
fn mark_export_print_requested(state: State<DatabaseState>, id: String) -> Result<(), String> {
    let conn = state
        .0
        .lock()
        .map_err(|_| "Database lock poisoned".to_string())?;
    let changed = conn
        .execute(
            "UPDATE export_manifests SET status='print_requested' WHERE id=?1",
            [&id],
        )
        .map_err(|error| error.to_string())?;
    if changed != 1 {
        return Err("Export manifest not found".into());
    }
    Ok(())
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let connection =
                initialize_database(app.handle()).map_err(|error| error.to_string())?;
            app.manage(DatabaseState(Mutex::new(connection)));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_store,
            save_store,
            save_task,
            delete_task,
            delete_record,
            update_record_body,
            update_task_status,
            save_user_protocol,
            save_protocol_template_version,
            start_task_record,
            get_assay_workspace,
            create_assay_plate,
            delete_empty_assay_plate,
            replace_assay_plate_mappings,
            upload_assay_raw_file,
            create_qpcr_delta_ct_analysis,
            create_qpcr_delta_delta_ct_analysis,
            user_data_location,
            export_workspace_backup,
            inspect_workspace_backup,
            restore_workspace_backup,
            create_export_manifest,
            mark_export_print_requested,
            create_process_event,
            apply_treatment_event,
            create_treatment_definition,
            update_treatment_definition,
            archive_treatment_definition,
            sample_detail,
            add_sample_alias,
            delete_sample_alias,
            delete_or_archive_sample,
            delete_or_archive_process_event,
            save_experiment,
            delete_experiment,
            create_container,
            delete_container,
            assign_sample_location,
            create_qpcr_mapping,
            delete_qpcr_mapping,
            lineage_workspace
        ])
        .run(tauri::generate_context!())
        .expect("error while running LabFlow")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        env,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn temporary_database_path(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        env::temp_dir().join(format!("labflow-{name}-{nonce}.sqlite"))
    }

    #[test]
    fn fresh_sqlite_migration_creates_required_tables() {
        let path = temporary_database_path("fresh");
        let connection = Connection::open(&path).unwrap();
        apply_schema(&connection).unwrap();
        let table_count: i64 = connection.query_row("SELECT count(*) FROM sqlite_master WHERE type='table' AND name IN ('experiments','tasks','protocols','records','samples','sample_types','attachments','export_manifests','assay_items','assay_plates','assay_well_mappings','assay_raw_imports','assay_raw_measurements')", [], |row| row.get(0)).unwrap();
        assert_eq!(table_count, 13);
        drop(connection);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn user_protocol_creation_registers_types_and_persists_v1_schema() {
        let path = temporary_database_path("user-protocol");
        let mut connection = Connection::open(&path).unwrap();
        apply_schema(&connection).unwrap();
        let saved = save_user_protocol_to_db(
            &mut connection,
            json!({
                "id":"protocol-mice-rna",
                "name":"Mice RNA extraction",
                "description":"Extract RNA from tissue",
                "category":"自定义",
                "accent":"#6957e8",
                "inputType":"Mice",
                "inputTypeDisplayName":"Mice",
                "outputBehavior":"derived_one",
                "outputType":"rna",
                "outputTypeDisplayName":"RNA",
                "consumptionPolicy":"consume",
                "template":"日期：{{date}}\n{{input_sample_summary}} -> {{output_sample_summary}}",
                "createdAt":"2026-08-25T10:00:00Z"
            }),
        )
        .unwrap();
        assert_eq!(saved["version"], 1);
        let schema: String = connection.query_row("SELECT schema_json FROM protocol_versions WHERE protocol_id='protocol-mice-rna' AND version_number=1 AND origin='user'",[],|row|row.get(0)).unwrap();
        let schema: Value = serde_json::from_str(&schema).unwrap();
        assert_eq!(schema["execution"]["engine"], "sample_flow_v1");
        assert_eq!(schema["execution"]["inputTypes"][0], "MICE");
        assert_eq!(schema["execution"]["outputType"], "RNA");
        let mice_display: String = connection
            .query_row(
                "SELECT display_name FROM sample_types WHERE canonical_type='MICE' AND origin='user'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(mice_display, "Mice");
        drop(connection);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn template_edit_creates_user_version_without_mutating_builtin_version() {
        let path = temporary_database_path("protocol-template-version");
        let mut connection = Connection::open(&path).unwrap();
        apply_schema(&connection).unwrap();
        ensure_builtin_protocols(&connection).unwrap();
        let original: String = connection
            .query_row(
                "SELECT schema_json FROM protocol_versions WHERE protocol_id='pro-rt' AND version_number=1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let saved = save_protocol_template_version_to_db(
            &mut connection,
            json!({
                "protocolId":"pro-rt",
                "template":"日期：{{date}}\n自定义正文\n输入：{{input_sample_summary}}\n输出：{{output_sample_summary}}\nRNA：{{rna_amount}}",
                "createdAt":"2026-08-25T10:00:00Z"
            }),
        )
        .unwrap();
        assert_eq!(saved["previousVersion"], 1);
        assert_eq!(saved["version"], 2);
        let (active, origin, active_schema): (i64, String, String) = connection.query_row("SELECT p.active_version,pv.origin,pv.schema_json FROM protocols p JOIN protocol_versions pv ON pv.protocol_id=p.id AND pv.version_number=p.active_version WHERE p.id='pro-rt'",[],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?))).unwrap();
        assert_eq!(active, 2);
        assert_eq!(origin, "user");
        assert!(active_schema.contains("自定义正文"));
        let unchanged: String = connection
            .query_row(
                "SELECT schema_json FROM protocol_versions WHERE protocol_id='pro-rt' AND version_number=1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(unchanged, original);
        drop(connection);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn migration_canonicalizes_sample_types_without_changing_sample_codes() {
        let path = temporary_database_path("sample-type-canonicalization");
        let connection = Connection::open(&path).unwrap();
        connection.execute_batch("CREATE TABLE experiments (id TEXT PRIMARY KEY, experiment_code TEXT NOT NULL UNIQUE, title TEXT NOT NULL, description TEXT NOT NULL, color TEXT NOT NULL); CREATE TABLE records (id TEXT PRIMARY KEY, task_id TEXT NOT NULL UNIQUE, experiment_id TEXT NOT NULL, protocol_id TEXT NOT NULL, protocol_snapshot_json TEXT NOT NULL, current_data_json TEXT NOT NULL, updated_at TEXT NOT NULL); CREATE TABLE samples (id TEXT PRIMARY KEY, workspace_id TEXT NOT NULL DEFAULT 'local', experiment_id TEXT NOT NULL, sample_code TEXT NOT NULL, sample_type TEXT NOT NULL, source_record_id TEXT, parent_sample_id TEXT, UNIQUE(workspace_id,sample_code)); INSERT INTO experiments VALUES ('exp','EXP900','Legacy','','#000'); INSERT INTO samples VALUES ('cdna','local','exp','EXP900-cDNA01','cDNA',NULL,NULL);").unwrap();
        apply_schema(&connection).unwrap();
        let (sample_code, sample_type): (String, String) = connection
            .query_row(
                "SELECT sample_code,sample_type FROM samples WHERE id='cdna'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(sample_code, "EXP900-cDNA01");
        assert_eq!(sample_type, "CDNA");
        let lowercase_insert = connection.execute("INSERT INTO samples (id,workspace_id,experiment_id,sample_code,sample_type) VALUES ('bad','local','exp','EXP900-cDNA02','cDNA')", []);
        assert!(lowercase_insert.is_err());
        drop(connection);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn migration_marks_imported_roots_external_without_reclassifying_internal_outputs() {
        let path = temporary_database_path("sample-origin-migration");
        let connection = Connection::open(&path).unwrap();
        apply_schema(&connection).unwrap();
        connection
            .execute(
                "INSERT INTO experiments VALUES ('exp','EXP900','Origins','','#000')",
                [],
            )
            .unwrap();
        connection.execute("INSERT INTO samples (id,experiment_id,sample_code,sample_type,created_at,metadata_json) VALUES ('external-root','exp','EXP900-CELL01','CELL','now','{}'),('internal-output','exp','EXP900-CELL02','CELL','now','{}')", []).unwrap();
        connection.execute("INSERT INTO process_events (id,experiment_id,event_type,occurred_at,parameters_json,provenance,created_at) VALUES ('imported','exp','import','now','{}','user_imported','now'),('recorded','exp','thaw','now','{}','labflow_recorded','now')", []).unwrap();
        connection.execute("INSERT INTO event_outputs VALUES ('imported','external-root'),('recorded','internal-output')", []).unwrap();
        apply_schema(&connection).unwrap();
        let origins: (String, String) = connection
            .query_row(
                "SELECT (SELECT origin FROM samples WHERE id='external-root'),(SELECT origin FROM samples WHERE id='internal-output')",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(origins, ("external".into(), "internal".into()));
        drop(connection);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn migration_backfills_passage_consumption_idempotently() {
        let path = temporary_database_path("passage-consumption");
        let connection = Connection::open(&path).unwrap();
        apply_schema(&connection).unwrap();
        connection
            .execute(
                "INSERT INTO experiments VALUES ('exp','EXP900','Passage migration','','#000')",
                [],
            )
            .unwrap();
        connection.execute("INSERT INTO samples (id,workspace_id,experiment_id,sample_code,sample_type,display_name,created_at,lineage_status,metadata_json) VALUES ('cell','local','exp','EXP900-CELL01','CELL','Cell','now','complete','{}')", []).unwrap();
        connection.execute("INSERT INTO process_events (id,experiment_id,event_type,occurred_at,parameters_json,provenance,created_at) VALUES ('passage','exp','passage','now','{}','labflow_recorded','now')", []).unwrap();
        connection
            .execute("INSERT INTO event_inputs VALUES ('passage','cell')", [])
            .unwrap();
        apply_schema(&connection).unwrap();
        apply_schema(&connection).unwrap();
        let usages: i64 = connection
            .query_row(
                "SELECT count(*) FROM sample_usages WHERE event_id='passage' AND sample_id='cell' AND usage_type='consumed'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(usages, 1);
        drop(connection);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn restart_reads_the_same_database() {
        let path = temporary_database_path("restart");
        let connection = Connection::open(&path).unwrap();
        apply_schema(&connection).unwrap();
        connection
            .execute(
                "INSERT INTO experiments VALUES ('persist','E900','Restart persistence','','#000')",
                [],
            )
            .unwrap();
        drop(connection);
        let reopened = Connection::open(&path).unwrap();
        let title: String = reopened
            .query_row(
                "SELECT title FROM experiments WHERE id='persist'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(title, "Restart persistence");
        drop(reopened);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn export_manifest_uses_task_time_and_frozen_record_content() {
        let path = temporary_database_path("export-manifest");
        let files_dir = path.with_extension("files");
        let mut connection = Connection::open(&path).unwrap();
        apply_schema(&connection).unwrap();
        seed_if_empty(&mut connection).unwrap();
        let result = create_export_manifest_at(
            &connection,
            &files_dir,
            &json!({
                "id":"export-test",
                "dateFrom":"2026-08-24",
                "dateTo":"2026-08-25",
                "recordIds":["record-template-passage","record-template-thaw"],
                "createdAt":"2026-08-24T12:00:00Z"
            }),
        )
        .unwrap();
        assert_eq!(result["recordCount"], 2);
        assert_eq!(result["contentSha256"].as_str().unwrap().len(), 64);
        let relative_path = result["relativePath"].as_str().unwrap();
        assert_eq!(relative_path, "files/exports/export-test/manifest.json");
        let manifest: Value = serde_json::from_slice(
            &fs::read(files_dir.join("exports/export-test/manifest.json")).unwrap(),
        )
        .unwrap();
        let recomputed_hash = format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&manifest["payload"]).unwrap())
        );
        assert_eq!(manifest["contentSha256"], recomputed_hash);
        assert_eq!(manifest["payload"]["records"][0]["taskTitle"], "细胞复苏");
        assert!(
            manifest["payload"]["records"][1]["currentData"]["renderedContent"]
                .as_str()
                .unwrap()
                .contains("PBS洗2～3次")
        );
        let status: String = connection
            .query_row(
                "SELECT status FROM export_manifests WHERE id='export-test'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(status, "previewed");
        drop(connection);
        fs::remove_file(path).unwrap();
        fs::remove_dir_all(files_dir).unwrap();
    }

    #[test]
    fn fresh_seed_contains_task_graph_and_sample_lineage_template() {
        let path = temporary_database_path("golden-path");
        let mut connection = Connection::open(&path).unwrap();
        apply_schema(&connection).unwrap();
        seed_if_empty(&mut connection).unwrap();
        let count: i64 = connection
            .query_row(
                "SELECT count(*) FROM samples WHERE experiment_id='exp-template'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 9);
        let task_relations: i64 = connection
            .query_row("SELECT count(*) FROM task_relations", [], |row| row.get(0))
            .unwrap();
        let sample_relations: i64 = connection
            .query_row("SELECT count(*) FROM sample_relations", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!((task_relations, sample_relations), (6, 8));
        let rna_parents = task_graph::parent_ids(&connection, "task-template-rna").unwrap();
        assert_eq!(rna_parents.len(), 2);
        assert!(rna_parents.contains(&"task-template-treatment".to_string()));
        assert!(rna_parents.contains(&"task-template-imaging".to_string()));
        let treatments =
            lineage::derived_treatments(&connection, "sample-template-well06").unwrap();
        assert_eq!(treatments.len(), 1);
        assert_eq!(treatments[0]["parameters"]["groups"][1]["factor"], "si 123");
        let passage_body: String = connection
            .query_row(
                "SELECT json_extract(current_data_json,'$.renderedContent') FROM records WHERE id='record-template-passage'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(passage_body.contains("PBS洗2～3次"));
        let passage_consumed: i64 = connection
            .query_row(
                "SELECT count(*) FROM sample_usages WHERE event_id='event-template-passage' AND sample_id='sample-template-cell01' AND usage_type='consumed'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(passage_consumed, 1);
        let experiments_before: i64 = connection
            .query_row("SELECT count(*) FROM experiments", [], |r| r.get(0))
            .unwrap();
        seed_if_empty(&mut connection).unwrap();
        let experiments_after: i64 = connection
            .query_row("SELECT count(*) FROM experiments", [], |r| r.get(0))
            .unwrap();
        assert_eq!(experiments_before, experiments_after);
        drop(connection);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn legacy_full_store_writer_cannot_overwrite_lineage_history() {
        let path = temporary_database_path("legacy-guard");
        let mut connection = Connection::open(&path).unwrap();
        apply_schema(&connection).unwrap();
        seed_if_empty(&mut connection).unwrap();
        assert!(write_store(&mut connection, json!({}))
            .unwrap_err()
            .contains("disabled"));
        drop(connection);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn operational_lineage_entities_survive_database_reopen() {
        let path = temporary_database_path("operational-persistence");
        let mut db = Connection::open(&path).unwrap();
        apply_schema(&db).unwrap();
        db.execute(
            "INSERT INTO experiments VALUES ('e','EXP010','Persistence','','#000')",
            [],
        )
        .unwrap();
        lineage::create_event(
            &mut db,
            lineage::Event {
                id: "thaw",
                experiment_id: "e",
                record_id: None,
                event_type: "thaw",
                occurred_at: "2026-08-23",
                parameters: json!({}),
                provenance: "labflow_recorded",
            },
            &[],
            &[lineage::NewSample {
                id: "cdna".into(),
                experiment_id: "e".into(),
                code: "EXP010-C001-cDNA01".into(),
                display_name: "cDNA1".into(),
                sample_type: "cdna".into(),
                lineage_status: "complete".into(),
                metadata: json!({}),
            }],
        )
        .unwrap();
        db.execute(
            "INSERT INTO sample_aliases VALUES ('a','cdna','legacy-cDNA','legacy','2026-08-23')",
            [],
        )
        .unwrap();
        db.execute(
            "INSERT INTO containers VALUES ('plate','e','plate','P001','{}','2026-08-23')",
            [],
        )
        .unwrap();
        db.execute(
            "INSERT INTO sample_locations VALUES ('loc','cdna','plate','A1','2026-08-23',NULL)",
            [],
        )
        .unwrap();
        db.execute("INSERT INTO treatment_definitions VALUES ('t','e','T01','siTiam1-1','{\"concentration\":\"50 nM\"}','2026-08-23',NULL)",[]).unwrap();
        db.execute(
            "INSERT INTO qpcr_plate_wells VALUES ('q','e','cdna','TIAM1',1,'A1','2026-08-23')",
            [],
        )
        .unwrap();
        drop(db);
        let reopened = Connection::open(&path).unwrap();
        for table in [
            "sample_aliases",
            "containers",
            "sample_locations",
            "treatment_definitions",
            "qpcr_plate_wells",
        ] {
            let count: i64 = reopened
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
                .unwrap();
            assert_eq!(count, 1, "{table}");
        }
        drop(reopened);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn task_schema_preserves_schedule_fields_across_reopen() {
        let path = temporary_database_path("task-persistence");
        let db = Connection::open(&path).unwrap();
        apply_schema(&db).unwrap();
        db.execute(
            "INSERT INTO experiments VALUES ('e','EXP100','Task test','','#000')",
            [],
        )
        .unwrap();
        assert!(validate_task(
            "  RNA extraction  ",
            "2026-08-24T09:00:00",
            "2026-08-24T11:00:00"
        )
        .is_ok());
        assert!(validate_task(" ", "2026-08-24T09:00:00", "2026-08-24T11:00:00").is_err());
        assert!(validate_task("bad", "2026-08-24T11:00:00", "2026-08-24T09:00:00").is_err());
        db.execute("INSERT INTO tasks (id,experiment_id,title,start_time,end_time,status,record_id,created_at,updated_at) VALUES ('t','e','RNA extraction','2026-08-24T09:00:00','2026-08-24T11:00:00','planned',NULL,'now','now')",[]).unwrap();
        drop(db);
        let reopened = Connection::open(&path).unwrap();
        let row: (String, String, String) = reopened
            .query_row(
                "SELECT title,start_time,end_time FROM tasks WHERE id='t'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(
            row,
            (
                "RNA extraction".into(),
                "2026-08-24T09:00:00".into(),
                "2026-08-24T11:00:00".into()
            )
        );
        drop(reopened);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn canonical_user_data_is_outside_source_tree() {
        let platform_data = PathBuf::from("/tmp/platform-data");
        let project = PathBuf::from("/tmp/project-source");
        let path = canonical_app_data_dir(platform_data);
        assert_eq!(path, PathBuf::from("/tmp/platform-data/LabFlow"));
        assert!(!path.starts_with(project));
    }

    #[test]
    fn lineage_resolves_split_pool_treatment_and_partial_history() {
        let path = temporary_database_path("lineage");
        let mut db = Connection::open(&path).unwrap();
        apply_schema(&db).unwrap();
        db.execute(
            "INSERT INTO experiments VALUES ('e','EXP001','Tiam1 siRNA screening','','#000')",
            [],
        )
        .unwrap();
        lineage::create_event(
            &mut db,
            lineage::Event {
                id: "thaw",
                experiment_id: "e",
                record_id: None,
                event_type: "thaw",
                occurred_at: "2026-08-20",
                parameters: json!({"cellLine":"A549"}),
                provenance: "labflow_recorded",
            },
            &[],
            &[lineage::NewSample {
                id: "c1".into(),
                experiment_id: "e".into(),
                code: "EXP001-C001".into(),
                display_name: "C001".into(),
                sample_type: "culture".into(),
                lineage_status: "complete".into(),
                metadata: json!({"cellLine":"A549"}),
            }],
        )
        .unwrap();
        lineage::create_event(
            &mut db,
            lineage::Event {
                id: "pass",
                experiment_id: "e",
                record_id: None,
                event_type: "passage",
                occurred_at: "2026-08-21",
                parameters: json!({}),
                provenance: "labflow_recorded",
            },
            &["c1"],
            &[
                lineage::NewSample {
                    id: "c2".into(),
                    experiment_id: "e".into(),
                    code: "EXP001-C002".into(),
                    display_name: "C002".into(),
                    sample_type: "culture".into(),
                    lineage_status: "complete".into(),
                    metadata: json!({}),
                },
                lineage::NewSample {
                    id: "c3".into(),
                    experiment_id: "e".into(),
                    code: "EXP001-C003".into(),
                    display_name: "C003".into(),
                    sample_type: "culture".into(),
                    lineage_status: "complete".into(),
                    metadata: json!({}),
                },
            ],
        )
        .unwrap();
        lineage::apply_treatment(
            &mut db,
            lineage::Event {
                id: "t1",
                experiment_id: "e",
                record_id: None,
                event_type: "treatment",
                occurred_at: "2026-08-22",
                parameters: json!({"name":"siTiam1-1","concentration":"50 nM"}),
                provenance: "labflow_recorded",
            },
            &["c2"],
        )
        .unwrap();
        lineage::create_event(
            &mut db,
            lineage::Event {
                id: "rna",
                experiment_id: "e",
                record_id: None,
                event_type: "rna_extraction",
                occurred_at: "2026-08-23",
                parameters: json!({}),
                provenance: "labflow_recorded",
            },
            &["c2"],
            &[lineage::NewSample {
                id: "rna1".into(),
                experiment_id: "e".into(),
                code: "EXP001-C002-RNA01".into(),
                display_name: "RNA001".into(),
                sample_type: "rna".into(),
                lineage_status: "complete".into(),
                metadata: json!({}),
            }],
        )
        .unwrap();
        lineage::create_event(
            &mut db,
            lineage::Event {
                id: "rt",
                experiment_id: "e",
                record_id: None,
                event_type: "reverse_transcription",
                occurred_at: "2026-08-24",
                parameters: json!({}),
                provenance: "labflow_recorded",
            },
            &["rna1"],
            &[lineage::NewSample {
                id: "cdna1".into(),
                experiment_id: "e".into(),
                code: "EXP001-C002-cDNA01".into(),
                display_name: "cDNA001".into(),
                sample_type: "cdna".into(),
                lineage_status: "complete".into(),
                metadata: json!({}),
            }],
        )
        .unwrap();
        let cdna_upstream = lineage::upstream(&db, "cdna1").unwrap();
        assert_eq!(cdna_upstream.len(), 3);
        assert!(cdna_upstream.contains(&"rna1".to_string()));
        assert!(cdna_upstream.contains(&"c2".to_string()));
        assert!(cdna_upstream.contains(&"c1".to_string()));
        assert_eq!(
            lineage::derived_treatments(&db, "cdna1").unwrap()[0]["parameters"]["concentration"],
            "50 nM"
        );
        lineage::create_event(
            &mut db,
            lineage::Event {
                id: "pool",
                experiment_id: "e",
                record_id: None,
                event_type: "pool",
                occurred_at: "2026-08-25",
                parameters: json!({}),
                provenance: "user_imported",
            },
            &["c2", "c3"],
            &[lineage::NewSample {
                id: "pool1".into(),
                experiment_id: "e".into(),
                code: "EXP001-C004".into(),
                display_name: "Imported pool".into(),
                sample_type: "culture".into(),
                lineage_status: "partial".into(),
                metadata: json!({}),
            }],
        )
        .unwrap();
        let pooled_upstream = lineage::upstream(&db, "pool1").unwrap();
        assert_eq!(pooled_upstream.len(), 3);
        assert!(pooled_upstream.contains(&"c1".to_string()));
        assert!(pooled_upstream.contains(&"c2".to_string()));
        assert!(pooled_upstream.contains(&"c3".to_string()));
        drop(db);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn delete_record_removes_owned_data_and_restores_task() {
        let path = temporary_database_path("delete-record");
        let files_dir = path.with_extension("files");
        let mut connection = Connection::open(&path).unwrap();
        apply_schema(&connection).unwrap();
        ensure_builtin_protocols(&connection).unwrap();
        connection
            .execute(
                "INSERT INTO experiments VALUES ('exp','EXP900','Delete record','','#000')",
                [],
            )
            .unwrap();
        connection.execute("INSERT INTO tasks (id,experiment_id,title,start_time,end_time,status,record_id,created_at,updated_at) VALUES ('task','exp','Thaw','2026-08-26T09:00','2026-08-26T10:00','planned',NULL,'now','now')", []).unwrap();
        let created = protocol_execution::execute(
            &mut connection,
            "task",
            "pro-cell-thaw",
            "record",
            json!({"cell_name":"A549"}),
            vec![],
        )
        .unwrap();
        fs::create_dir_all(files_dir.join("attachment")).unwrap();
        fs::write(files_dir.join("attachment/raw.txt"), b"raw").unwrap();
        connection.execute("INSERT INTO attachments (id,record_id,file_name,relative_path,created_at) VALUES ('attachment','record','raw.txt','files/attachment/raw.txt','now')", []).unwrap();
        connection.execute("INSERT INTO results (id,record_id,result_type,structured_data_json,created_at) VALUES ('result','record','test','{}','now')", []).unwrap();
        connection.execute("INSERT INTO export_manifests (id,date_from,date_to,record_ids_json,content_sha256,relative_path,status,created_at) VALUES ('export','2026-08-26','2026-08-26','[\"record\"]','hash','files/exports/export/manifest.json','previewed','now')", []).unwrap();

        let export_error =
            delete_record_from_db(&mut connection, &files_dir, "record").unwrap_err();
        assert!(export_error.contains("export manifest"));
        connection
            .execute("DELETE FROM export_manifests WHERE id='export'", [])
            .unwrap();

        delete_record_from_db(&mut connection, &files_dir, "record").unwrap();

        let task: (String, Option<String>) = connection
            .query_row(
                "SELECT status,record_id FROM tasks WHERE id='task'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(task, ("planned".into(), None));
        for (table, predicate) in [
            ("records", "id='record'"),
            ("process_events", "record_id='record'"),
            ("record_samples", "record_id='record'"),
            ("attachments", "record_id='record'"),
            ("results", "record_id='record'"),
        ] {
            let count: i64 = connection
                .query_row(
                    &format!("SELECT count(*) FROM {table} WHERE {predicate}"),
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(count, 0, "{table} was not cleaned");
        }
        let output_count: i64 = connection
            .query_row(
                "SELECT count(*) FROM samples WHERE id=?1",
                [&created.output_ids[0]],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(output_count, 0);
        assert!(!files_dir.join("attachment").exists());
        drop(connection);
        fs::remove_file(path).unwrap();
        fs::remove_dir_all(files_dir).unwrap();
    }

    #[test]
    fn record_body_edit_is_scoped_audited_and_atomic() {
        let path = temporary_database_path("record-body-edit");
        let mut connection = Connection::open(&path).unwrap();
        apply_schema(&connection).unwrap();
        ensure_builtin_protocols(&connection).unwrap();
        connection
            .execute(
                "INSERT INTO experiments VALUES ('exp','EXP901','Record body edit','','#000')",
                [],
            )
            .unwrap();
        connection.execute("INSERT INTO tasks (id,experiment_id,title,start_time,end_time,status,record_id,created_at,updated_at) VALUES ('task-a','exp','Thaw A','2026-08-26T09:00','2026-08-26T10:00','planned',NULL,'now','now'),('task-b','exp','Thaw B','2026-08-26T10:00','2026-08-26T11:00','planned',NULL,'now','now')", []).unwrap();
        protocol_execution::execute(
            &mut connection,
            "task-a",
            "pro-cell-thaw",
            "record-a",
            json!({"cell_name":"A549"}),
            vec![],
        )
        .unwrap();
        protocol_execution::execute(
            &mut connection,
            "task-b",
            "pro-cell-thaw",
            "record-b",
            json!({"cell_name":"HeLa"}),
            vec![],
        )
        .unwrap();
        let snapshot_before: String = connection
            .query_row(
                "SELECT protocol_snapshot_json FROM records WHERE id='record-a'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let sibling_before: String = connection
            .query_row(
                "SELECT current_data_json FROM records WHERE id='record-b'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        update_record_body_in_db(
            &mut connection,
            "record-a",
            "Edited procedure\n1. Keep exact text.",
            "record-change-a",
            "2026-08-26T12:00:00Z",
        )
        .unwrap();

        let (current_after, snapshot_after, updated_at): (String, String, String) = connection
            .query_row(
                "SELECT current_data_json,protocol_snapshot_json,updated_at FROM records WHERE id='record-a'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        let current_after: Value = serde_json::from_str(&current_after).unwrap();
        assert_eq!(
            current_after["renderedContent"],
            "Edited procedure\n1. Keep exact text."
        );
        assert_eq!(snapshot_after, snapshot_before);
        assert_eq!(updated_at, "2026-08-26T12:00:00Z");
        assert_eq!(
            connection
                .query_row(
                    "SELECT current_data_json FROM records WHERE id='record-b'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .unwrap(),
            sibling_before
        );
        let change: (String, String, String) = connection
            .query_row(
                "SELECT field_path,old_value_json,new_value_json FROM record_changes WHERE id='record-change-a'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(change.0, "renderedContent");
        assert!(change.1.contains("37"));
        assert_eq!(
            serde_json::from_str::<Value>(&change.2).unwrap(),
            "Edited procedure\n1. Keep exact text."
        );

        assert!(update_record_body_in_db(
            &mut connection,
            "record-a",
            "   ",
            "record-change-blank",
            "2026-08-26T13:00:00Z",
        )
        .unwrap_err()
        .contains("cannot be empty"));
        assert!(update_record_body_in_db(
            &mut connection,
            "record-a",
            "This update must roll back",
            "record-change-a",
            "2026-08-26T14:00:00Z",
        )
        .is_err());
        let preserved: String = connection
            .query_row(
                "SELECT json_extract(current_data_json,'$.renderedContent') FROM records WHERE id='record-a'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(preserved, "Edited procedure\n1. Keep exact text.");
        drop(connection);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn delete_record_is_blocked_when_an_output_has_downstream_use() {
        let path = temporary_database_path("delete-record-downstream");
        let files_dir = path.with_extension("files");
        let mut connection = Connection::open(&path).unwrap();
        apply_schema(&connection).unwrap();
        ensure_builtin_protocols(&connection).unwrap();
        connection
            .execute(
                "INSERT INTO experiments VALUES ('exp','EXP900','Delete record','','#000')",
                [],
            )
            .unwrap();
        connection.execute("INSERT INTO tasks (id,experiment_id,title,start_time,end_time,status,record_id,created_at,updated_at) VALUES ('source-task','exp','Thaw','2026-08-26T09:00','2026-08-26T10:00','planned',NULL,'now','now'),('child-task','exp','Use','2026-08-27T09:00','2026-08-27T10:00','in_progress','child-record','now','now')", []).unwrap();
        let source = protocol_execution::execute(
            &mut connection,
            "source-task",
            "pro-cell-thaw",
            "source-record",
            json!({"cell_name":"A549"}),
            vec![],
        )
        .unwrap();
        connection.execute("INSERT INTO records (id,task_id,experiment_id,protocol_id,protocol_snapshot_json,current_data_json,updated_at) VALUES ('child-record','child-task','exp','pro-cell-passage','{}','{}','now')", []).unwrap();
        connection
            .execute(
                "INSERT INTO record_samples VALUES ('child-record',?1,'input')",
                [&source.output_ids[0]],
            )
            .unwrap();

        let error =
            delete_record_from_db(&mut connection, &files_dir, "source-record").unwrap_err();
        assert!(error.contains("downstream"));
        let preserved: i64 = connection
            .query_row(
                "SELECT count(*) FROM records WHERE id='source-record'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(preserved, 1);
        drop(connection);
        fs::remove_file(path).unwrap();
    }
}
