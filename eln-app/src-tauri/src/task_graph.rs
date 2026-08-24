use rusqlite::{params, Connection, Transaction};
use std::collections::HashSet;

#[allow(dead_code)]
pub fn parent_ids(connection: &Connection, task_id: &str) -> Result<Vec<String>, String> {
    let mut statement = connection
        .prepare(
            "SELECT parent_task_id FROM task_relations WHERE child_task_id=?1 ORDER BY created_at,id",
        )
        .map_err(|error| error.to_string())?;
    let result = statement
        .query_map([task_id], |row| row.get(0))
        .map_err(|error| error.to_string())?
        .map(|row| row.map_err(|error| error.to_string()))
        .collect();
    result
}

fn task_reaches(connection: &Connection, start: &str, target: &str) -> Result<bool, String> {
    connection
        .query_row(
            "WITH RECURSIVE descendants(id) AS (
               SELECT child_task_id FROM task_relations WHERE parent_task_id=?1
               UNION
               SELECT relation.child_task_id
               FROM task_relations relation
               JOIN descendants ON relation.parent_task_id=descendants.id
             )
             SELECT EXISTS(SELECT 1 FROM descendants WHERE id=?2)",
            params![start, target],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())
}

pub fn replace_parents(
    transaction: &Transaction<'_>,
    task_id: &str,
    experiment_id: &str,
    parent_task_ids: &[String],
    changed_at: &str,
) -> Result<(), String> {
    let mut unique = HashSet::new();
    for parent_id in parent_task_ids {
        if !unique.insert(parent_id.as_str()) {
            return Err("The same parent Task cannot be selected twice".into());
        }
        if parent_id == task_id {
            return Err("A Task cannot depend on itself".into());
        }
        let belongs_to_experiment: bool = transaction
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM tasks WHERE id=?1 AND experiment_id=?2)",
                params![parent_id, experiment_id],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        if !belongs_to_experiment {
            return Err("Every parent Task must belong to the same Experiment".into());
        }
    }

    transaction
        .execute(
            "DELETE FROM task_relations WHERE child_task_id=?1",
            [task_id],
        )
        .map_err(|error| error.to_string())?;

    for parent_id in parent_task_ids {
        if task_reaches(transaction, task_id, parent_id)? {
            return Err("Task dependency would create a cycle".into());
        }
        transaction
            .execute(
                "INSERT INTO task_relations (id,experiment_id,parent_task_id,child_task_id,relation_type,created_at)
                 VALUES (?1,?2,?3,?4,'depends_on',?5)",
                params![
                    format!("task-relation-{parent_id}-{task_id}"),
                    experiment_id,
                    parent_id,
                    task_id,
                    changed_at
                ],
            )
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

pub fn has_children(connection: &Connection, task_id: &str) -> Result<bool, String> {
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM task_relations WHERE parent_task_id=?1)",
            [task_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn database() -> Connection {
        let connection = Connection::open_in_memory().unwrap();
        crate::apply_schema(&connection).unwrap();
        connection
            .execute(
                "INSERT INTO experiments VALUES ('e','EXP100','Graph','','#000')",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO experiments VALUES ('other','EXP101','Other','','#000')",
                [],
            )
            .unwrap();
        for (id, experiment) in [
            ("a", "e"),
            ("b", "e"),
            ("c", "e"),
            ("d", "e"),
            ("x", "other"),
        ] {
            connection.execute("INSERT INTO tasks (id,experiment_id,title,start_time,end_time,status,created_at,updated_at) VALUES (?1,?2,?1,'2026-08-24T08:00','2026-08-24T09:00','planned','now','now')",params![id,experiment]).unwrap();
        }
        connection
    }

    #[test]
    fn supports_branch_merge_and_rejects_cycles() {
        let mut database = database();
        {
            let transaction = database.transaction().unwrap();
            replace_parents(&transaction, "b", "e", &["a".into()], "now").unwrap();
            replace_parents(&transaction, "c", "e", &["b".into()], "now").unwrap();
            replace_parents(&transaction, "d", "e", &["b".into(), "c".into()], "now").unwrap();
            transaction.commit().unwrap();
        }
        assert_eq!(parent_ids(&database, "d").unwrap().len(), 2);
        let transaction = database.transaction().unwrap();
        assert!(
            replace_parents(&transaction, "a", "e", &["c".into()], "now")
                .unwrap_err()
                .contains("cycle")
        );
    }

    #[test]
    fn rejects_cross_experiment_and_self_dependencies() {
        let mut database = database();
        let transaction = database.transaction().unwrap();
        assert!(
            replace_parents(&transaction, "a", "e", &["x".into()], "now")
                .unwrap_err()
                .contains("same Experiment")
        );
        assert!(
            replace_parents(&transaction, "a", "e", &["a".into()], "now")
                .unwrap_err()
                .contains("itself")
        );
    }
}
