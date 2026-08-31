import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import Database from "better-sqlite3";
import { openDatabase } from "../server/database.ts";

test("web compatibility layer adds Protocol and image attachment columns to an existing database", () => {
  const directory = mkdtempSync(join(tmpdir(), "labflow-protocol-schema-"));
  const path = join(directory, "legacy.sqlite");
  const legacy = new Database(path);
  legacy.exec(`
    CREATE TABLE protocols (id TEXT PRIMARY KEY, name TEXT NOT NULL, category TEXT NOT NULL, active_version INTEGER NOT NULL, accent TEXT NOT NULL);
    CREATE TABLE protocol_versions (protocol_id TEXT NOT NULL REFERENCES protocols(id), version_number INTEGER NOT NULL, schema_json TEXT NOT NULL, PRIMARY KEY(protocol_id, version_number));
    CREATE TABLE samples (id TEXT PRIMARY KEY, workspace_id TEXT NOT NULL DEFAULT 'local', experiment_id TEXT NOT NULL, sample_code TEXT NOT NULL, sample_type TEXT NOT NULL, source_record_id TEXT, parent_sample_id TEXT, UNIQUE(workspace_id, sample_code));
    CREATE TABLE attachments (id TEXT PRIMARY KEY, record_id TEXT NOT NULL, file_name TEXT NOT NULL, relative_path TEXT NOT NULL, mime_type TEXT, size INTEGER, created_at TEXT NOT NULL);
  `);
  legacy.close();

  const migrated = openDatabase(path);
  const protocolColumns = new Set(
    (migrated.prepare("PRAGMA table_info(protocols)").all() as Array<{ name: string }>).map(
      (item) => item.name,
    ),
  );
  const versionColumns = new Set(
    (
      migrated.prepare("PRAGMA table_info(protocol_versions)").all() as Array<{
        name: string;
      }>
    ).map((item) => item.name),
  );
  assert(protocolColumns.has("description"));
  assert(protocolColumns.has("origin"));
  assert(versionColumns.has("origin"));
  assert(versionColumns.has("created_at"));
  const sampleColumns = new Set(
    (migrated.prepare("PRAGMA table_info(samples)").all() as Array<{ name: string }>).map(
      (item) => item.name,
    ),
  );
  assert(sampleColumns.has("origin"));
  const attachmentColumns = new Set(
    (migrated.prepare("PRAGMA table_info(attachments)").all() as Array<{ name: string }>).map(
      (item) => item.name,
    ),
  );
  assert(attachmentColumns.has("content_sha256"));
  assert(attachmentColumns.has("preview_relative_path"));
  assert(attachmentColumns.has("width_px"));
  assert(attachmentColumns.has("height_px"));
  migrated.close();
  rmSync(directory, { recursive: true, force: true });
});
