use rusqlite::{params, Connection};
use serde_json::{json, Value};

pub struct NewSample {
    pub id: String,
    pub experiment_id: String,
    pub code: String,
    pub display_name: String,
    pub sample_type: String,
    pub lineage_status: String,
    pub metadata: Value,
}
pub struct Event<'a> {
    pub id: &'a str,
    pub experiment_id: &'a str,
    pub record_id: Option<&'a str>,
    pub event_type: &'a str,
    pub occurred_at: &'a str,
    pub parameters: Value,
    pub provenance: &'a str,
}

pub fn create_event(
    connection: &mut Connection,
    event: Event<'_>,
    inputs: &[&str],
    outputs: &[NewSample],
) -> Result<(), String> {
    let tx = connection.transaction().map_err(|e| e.to_string())?;
    tx.execute("INSERT INTO process_events (id,experiment_id,record_id,event_type,occurred_at,parameters_json,provenance,created_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?5)", params![event.id,event.experiment_id,event.record_id,event.event_type,event.occurred_at,event.parameters.to_string(),event.provenance]).map_err(|e|e.to_string())?;
    for input in inputs {
        tx.execute(
            "INSERT INTO event_inputs VALUES (?1,?2)",
            params![event.id, input],
        )
        .map_err(|e| e.to_string())?;
    }
    for output in outputs {
        let parent = if inputs.len() == 1 {
            Some(inputs[0])
        } else {
            None
        };
        let sample_type = output.sample_type.to_uppercase();
        tx.execute("INSERT INTO samples (id,workspace_id,experiment_id,sample_code,sample_type,source_record_id,parent_sample_id,display_name,created_at,lineage_status,metadata_json) VALUES (?1,'local',?2,?3,?4,?5,?6,?7,?8,?9,?10)", params![output.id,output.experiment_id,output.code,sample_type,event.record_id,parent,output.display_name,event.occurred_at,output.lineage_status,output.metadata.to_string()]).map_err(|e|e.to_string())?;
        tx.execute(
            "INSERT INTO event_outputs VALUES (?1,?2)",
            params![event.id, output.id],
        )
        .map_err(|e| e.to_string())?;
        if let Some(parent) = parent {
            tx.execute("INSERT OR IGNORE INTO sample_relations (id,parent_sample_id,child_sample_id,relation_type) VALUES (?1,?2,?3,'derived_from')", params![format!("relation-{}",output.id),parent,output.id]).map_err(|e|e.to_string())?;
        }
    }
    tx.commit().map_err(|e| e.to_string())
}

/// A treatment changes material state without changing Sample identity.
pub fn apply_treatment(
    connection: &mut Connection,
    event: Event<'_>,
    sample_ids: &[&str],
) -> Result<(), String> {
    let tx = connection.transaction().map_err(|e| e.to_string())?;
    tx.execute("INSERT INTO process_events (id,experiment_id,record_id,event_type,occurred_at,parameters_json,provenance,created_at) VALUES (?1,?2,?3,'treatment',?4,?5,?6,?4)", params![event.id,event.experiment_id,event.record_id,event.occurred_at,event.parameters.to_string(),event.provenance]).map_err(|e|e.to_string())?;
    for sample_id in sample_ids {
        tx.execute(
            "INSERT INTO event_inputs VALUES (?1,?2)",
            params![event.id, sample_id],
        )
        .map_err(|e| e.to_string())?;
        tx.execute(
            "INSERT INTO event_outputs VALUES (?1,?2)",
            params![event.id, sample_id],
        )
        .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())
}

pub fn audit(
    connection: &Connection,
    id: &str,
    entity_type: &str,
    entity_id: &str,
    field: &str,
    old: Value,
    new: Value,
    at: &str,
) -> Result<(), String> {
    connection
        .execute(
            "INSERT INTO entity_changes VALUES (?1,?2,?3,?4,?5,?6,'local_user',?7)",
            params![
                id,
                entity_type,
                entity_id,
                field,
                old.to_string(),
                new.to_string(),
                at
            ],
        )
        .map(|_| ())
        .map_err(|e| e.to_string())
}

pub fn derived_treatments(connection: &Connection, sample_id: &str) -> Result<Vec<Value>, String> {
    let mut statement = connection.prepare("WITH RECURSIVE upstream(sample_id) AS (SELECT ?1 UNION SELECT ei.sample_id FROM event_outputs eo JOIN event_inputs ei ON ei.event_id=eo.event_id JOIN upstream u ON eo.sample_id=u.sample_id) SELECT e.id,e.occurred_at,e.parameters_json,e.provenance FROM process_events e JOIN event_outputs eo ON eo.event_id=e.id JOIN upstream u ON eo.sample_id=u.sample_id WHERE e.event_type='treatment' ORDER BY e.occurred_at").map_err(|e|e.to_string())?;
    let rows=statement.query_map([sample_id],|row|{let p:String=row.get(2)?;Ok(json!({"eventId":row.get::<_,String>(0)?,"occurredAt":row.get::<_,String>(1)?,"parameters":serde_json::from_str::<Value>(&p).unwrap_or(json!({})),"provenance":row.get::<_,String>(3)?}))}).map_err(|e|e.to_string())?;
    rows.map(|row| row.map_err(|e| e.to_string())).collect()
}

pub fn upstream(connection: &Connection, sample_id: &str) -> Result<Vec<String>, String> {
    let mut statement=connection.prepare("WITH RECURSIVE u(id) AS (SELECT ?1 UNION SELECT ei.sample_id FROM u JOIN event_outputs eo ON eo.sample_id=u.id JOIN event_inputs ei ON ei.event_id=eo.event_id) SELECT id FROM u WHERE id<>?1").map_err(|e|e.to_string())?;
    let rows = statement
        .query_map([sample_id], |row| row.get(0))
        .map_err(|e| e.to_string())?;
    rows.map(|row| row.map_err(|e| e.to_string())).collect()
}
