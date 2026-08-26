import Database from "better-sqlite3";
import { existsSync } from "node:fs";

const requiredTables = [
  "experiments",
  "tasks",
  "protocols",
  "protocol_versions",
  "records",
  "record_changes",
  "samples",
  "sample_relations",
  "record_samples",
  "attachments",
  "results",
];

export function validateDatabase(databasePath: string) {
  const db = new Database(databasePath, { readonly: true });
  try {
    if ((db.pragma("integrity_check", { simple: true }) as string) !== "ok")
      throw new Error("SQLite integrity_check failed");
    const tables = new Set(
      (
        db
          .prepare("SELECT name FROM sqlite_master WHERE type = 'table'")
          .all() as Array<{ name: string }>
      ).map((row) => row.name),
    );
    for (const table of requiredTables)
      if (!tables.has(table)) throw new Error(`Missing table: ${table}`);
  } finally {
    db.close();
  }
}

export function openDatabase(databasePath: string) {
  const db = new Database(databasePath);
  db.pragma("foreign_keys = ON");
  db.exec(`
    CREATE TABLE IF NOT EXISTS experiments (id TEXT PRIMARY KEY, experiment_code TEXT NOT NULL UNIQUE, title TEXT NOT NULL, description TEXT NOT NULL, color TEXT NOT NULL);
    CREATE TABLE IF NOT EXISTS tasks (id TEXT PRIMARY KEY, experiment_id TEXT NOT NULL REFERENCES experiments(id), title TEXT NOT NULL, start_time TEXT NOT NULL, end_time TEXT NOT NULL, status TEXT NOT NULL CHECK(status IN ('planned','in_progress','completed')), record_id TEXT);
    CREATE TABLE IF NOT EXISTS protocols (id TEXT PRIMARY KEY, name TEXT NOT NULL, category TEXT NOT NULL, active_version INTEGER NOT NULL, accent TEXT NOT NULL, description TEXT NOT NULL DEFAULT '', origin TEXT NOT NULL DEFAULT 'builtin');
    CREATE TABLE IF NOT EXISTS protocol_versions (protocol_id TEXT NOT NULL REFERENCES protocols(id), version_number INTEGER NOT NULL, schema_json TEXT NOT NULL, origin TEXT NOT NULL DEFAULT 'builtin', created_at TEXT, PRIMARY KEY(protocol_id, version_number));
    CREATE TABLE IF NOT EXISTS records (id TEXT PRIMARY KEY, task_id TEXT NOT NULL UNIQUE REFERENCES tasks(id), experiment_id TEXT NOT NULL REFERENCES experiments(id), protocol_id TEXT NOT NULL REFERENCES protocols(id), protocol_snapshot_json TEXT NOT NULL, current_data_json TEXT NOT NULL, updated_at TEXT NOT NULL);
    CREATE TABLE IF NOT EXISTS record_changes (id TEXT PRIMARY KEY, record_id TEXT NOT NULL REFERENCES records(id), field_path TEXT NOT NULL, old_value_json TEXT NOT NULL, new_value_json TEXT NOT NULL, actor_id TEXT NOT NULL DEFAULT 'local_user', changed_at TEXT NOT NULL);
    CREATE TABLE IF NOT EXISTS samples (id TEXT PRIMARY KEY, workspace_id TEXT NOT NULL DEFAULT 'local', experiment_id TEXT NOT NULL REFERENCES experiments(id), sample_code TEXT NOT NULL, sample_type TEXT NOT NULL, source_record_id TEXT REFERENCES records(id), parent_sample_id TEXT, origin TEXT NOT NULL DEFAULT 'internal' CHECK(origin IN ('internal','external')), UNIQUE(workspace_id, sample_code));
    CREATE TABLE IF NOT EXISTS sample_types (canonical_type TEXT PRIMARY KEY, display_name TEXT NOT NULL, origin TEXT NOT NULL, created_at TEXT NOT NULL, archived_at TEXT);
    CREATE TABLE IF NOT EXISTS sample_relations (id TEXT PRIMARY KEY, parent_sample_id TEXT NOT NULL REFERENCES samples(id), child_sample_id TEXT NOT NULL REFERENCES samples(id), relation_type TEXT NOT NULL, UNIQUE(parent_sample_id, child_sample_id, relation_type));
    CREATE TABLE IF NOT EXISTS record_samples (record_id TEXT NOT NULL REFERENCES records(id), sample_id TEXT NOT NULL REFERENCES samples(id), role TEXT NOT NULL CHECK(role IN ('input','output')), PRIMARY KEY(record_id, sample_id, role));
    CREATE TABLE IF NOT EXISTS attachments (id TEXT PRIMARY KEY, record_id TEXT NOT NULL REFERENCES records(id), file_name TEXT NOT NULL, relative_path TEXT NOT NULL CHECK(relative_path NOT LIKE '/%'), mime_type TEXT, size INTEGER, created_at TEXT NOT NULL);
    CREATE TABLE IF NOT EXISTS results (id TEXT PRIMARY KEY, record_id TEXT NOT NULL REFERENCES records(id), result_type TEXT NOT NULL, structured_data_json TEXT NOT NULL, created_at TEXT NOT NULL);
  `);
  const ensureColumn = (table: string, column: string, definition: string) => {
    const columns = new Set(
      (
        db.prepare(`PRAGMA table_info(${table})`).all() as Array<{
          name: string;
        }>
      ).map((item) => item.name),
    );
    if (!columns.has(column)) db.exec(`ALTER TABLE ${table} ADD COLUMN ${definition}`);
  };
  ensureColumn("protocols", "description", "description TEXT NOT NULL DEFAULT ''");
  ensureColumn("protocols", "origin", "origin TEXT NOT NULL DEFAULT 'builtin'");
  ensureColumn(
    "protocol_versions",
    "origin",
    "origin TEXT NOT NULL DEFAULT 'builtin'",
  );
  ensureColumn("protocol_versions", "created_at", "created_at TEXT");
  ensureColumn(
    "samples",
    "origin",
    "origin TEXT NOT NULL DEFAULT 'internal' CHECK(origin IN ('internal','external'))",
  );
  return db;
}

export function databaseExistsOutsideProject(
  databasePath: string,
  projectRoot: string,
) {
  return (
    existsSync(databasePath) && !databasePath.startsWith(`${projectRoot}/`)
  );
}
