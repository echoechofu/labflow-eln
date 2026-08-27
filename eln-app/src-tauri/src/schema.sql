CREATE TABLE IF NOT EXISTS experiments (id TEXT PRIMARY KEY, experiment_code TEXT NOT NULL UNIQUE, title TEXT NOT NULL, description TEXT NOT NULL, color TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS tasks (id TEXT PRIMARY KEY, experiment_id TEXT NOT NULL REFERENCES experiments(id), title TEXT NOT NULL, start_time TEXT NOT NULL, end_time TEXT NOT NULL, status TEXT NOT NULL CHECK(status IN ('planned','in_progress','completed')), record_id TEXT);
CREATE UNIQUE INDEX IF NOT EXISTS tasks_id_experiment_unique ON tasks(id, experiment_id);
CREATE TABLE IF NOT EXISTS task_relations (
  id TEXT PRIMARY KEY,
  experiment_id TEXT NOT NULL REFERENCES experiments(id),
  parent_task_id TEXT NOT NULL,
  child_task_id TEXT NOT NULL,
  relation_type TEXT NOT NULL DEFAULT 'depends_on' CHECK(relation_type IN ('depends_on')),
  created_at TEXT NOT NULL,
  UNIQUE(parent_task_id, child_task_id, relation_type),
  CHECK(parent_task_id <> child_task_id),
  FOREIGN KEY(parent_task_id, experiment_id) REFERENCES tasks(id, experiment_id),
  FOREIGN KEY(child_task_id, experiment_id) REFERENCES tasks(id, experiment_id)
);
CREATE INDEX IF NOT EXISTS task_relations_parent_index ON task_relations(parent_task_id);
CREATE INDEX IF NOT EXISTS task_relations_child_index ON task_relations(child_task_id);
CREATE TABLE IF NOT EXISTS protocols (id TEXT PRIMARY KEY, name TEXT NOT NULL, category TEXT NOT NULL, active_version INTEGER NOT NULL, accent TEXT NOT NULL, description TEXT NOT NULL DEFAULT '', origin TEXT NOT NULL DEFAULT 'builtin' CHECK(origin IN ('builtin','user')));
CREATE TABLE IF NOT EXISTS protocol_versions (protocol_id TEXT NOT NULL REFERENCES protocols(id), version_number INTEGER NOT NULL, schema_json TEXT NOT NULL, origin TEXT NOT NULL DEFAULT 'builtin' CHECK(origin IN ('builtin','user')), created_at TEXT, PRIMARY KEY(protocol_id, version_number));
CREATE TABLE IF NOT EXISTS records (id TEXT PRIMARY KEY, task_id TEXT NOT NULL UNIQUE REFERENCES tasks(id), experiment_id TEXT NOT NULL REFERENCES experiments(id), protocol_id TEXT NOT NULL, protocol_snapshot_json TEXT NOT NULL, current_data_json TEXT NOT NULL, updated_at TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS record_changes (id TEXT PRIMARY KEY, record_id TEXT NOT NULL REFERENCES records(id), field_path TEXT NOT NULL, old_value_json TEXT NOT NULL, new_value_json TEXT NOT NULL, actor_id TEXT NOT NULL DEFAULT 'local_user', changed_at TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS samples (id TEXT PRIMARY KEY, workspace_id TEXT NOT NULL DEFAULT 'local', experiment_id TEXT NOT NULL REFERENCES experiments(id), sample_code TEXT NOT NULL, sample_type TEXT NOT NULL, source_record_id TEXT REFERENCES records(id), parent_sample_id TEXT, origin TEXT NOT NULL DEFAULT 'internal' CHECK(origin IN ('internal','external')), UNIQUE(workspace_id, sample_code));
CREATE TABLE IF NOT EXISTS sample_types (
  canonical_type TEXT PRIMARY KEY,
  display_name TEXT NOT NULL,
  origin TEXT NOT NULL CHECK(origin IN ('builtin','user')),
  created_at TEXT NOT NULL,
  archived_at TEXT,
  CHECK(canonical_type = upper(canonical_type))
);
CREATE TRIGGER IF NOT EXISTS samples_type_uppercase_insert BEFORE INSERT ON samples WHEN NEW.sample_type <> upper(NEW.sample_type) BEGIN SELECT RAISE(ABORT, 'sample_type must use its uppercase canonical value'); END;
CREATE TRIGGER IF NOT EXISTS samples_type_uppercase_update BEFORE UPDATE OF sample_type ON samples WHEN NEW.sample_type <> upper(NEW.sample_type) BEGIN SELECT RAISE(ABORT, 'sample_type must use its uppercase canonical value'); END;
CREATE TABLE IF NOT EXISTS sample_relations (id TEXT PRIMARY KEY, parent_sample_id TEXT NOT NULL REFERENCES samples(id), child_sample_id TEXT NOT NULL REFERENCES samples(id), relation_type TEXT NOT NULL, UNIQUE(parent_sample_id, child_sample_id, relation_type));
CREATE TABLE IF NOT EXISTS record_samples (record_id TEXT NOT NULL REFERENCES records(id), sample_id TEXT NOT NULL REFERENCES samples(id), role TEXT NOT NULL CHECK(role IN ('input','output')), PRIMARY KEY(record_id, sample_id, role));
CREATE TABLE IF NOT EXISTS attachments (id TEXT PRIMARY KEY, record_id TEXT NOT NULL REFERENCES records(id), file_name TEXT NOT NULL, relative_path TEXT NOT NULL CHECK(relative_path NOT LIKE '/%'), mime_type TEXT, size INTEGER, created_at TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS results (id TEXT PRIMARY KEY, record_id TEXT NOT NULL REFERENCES records(id), result_type TEXT NOT NULL, structured_data_json TEXT NOT NULL, created_at TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS sample_aliases (id TEXT PRIMARY KEY, sample_id TEXT NOT NULL REFERENCES samples(id), alias TEXT NOT NULL, alias_type TEXT NOT NULL, created_at TEXT NOT NULL, UNIQUE(sample_id, alias));
CREATE TABLE IF NOT EXISTS containers (id TEXT PRIMARY KEY, experiment_id TEXT NOT NULL REFERENCES experiments(id), container_type TEXT NOT NULL, name TEXT NOT NULL, metadata_json TEXT NOT NULL DEFAULT '{}', created_at TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS sample_locations (id TEXT PRIMARY KEY, sample_id TEXT NOT NULL REFERENCES samples(id), container_id TEXT NOT NULL REFERENCES containers(id), position TEXT NOT NULL, valid_from TEXT NOT NULL, valid_until TEXT);
CREATE TABLE IF NOT EXISTS treatment_definitions (id TEXT PRIMARY KEY, experiment_id TEXT NOT NULL REFERENCES experiments(id), short_code TEXT NOT NULL, name TEXT NOT NULL, parameters_json TEXT NOT NULL DEFAULT '{}', created_at TEXT NOT NULL, archived_at TEXT, UNIQUE(experiment_id, short_code));
CREATE TABLE IF NOT EXISTS process_events (id TEXT PRIMARY KEY, experiment_id TEXT NOT NULL REFERENCES experiments(id), record_id TEXT REFERENCES records(id), event_type TEXT NOT NULL, occurred_at TEXT NOT NULL, parameters_json TEXT NOT NULL DEFAULT '{}', provenance TEXT NOT NULL CHECK(provenance IN ('labflow_recorded','user_imported')), created_at TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS event_inputs (event_id TEXT NOT NULL REFERENCES process_events(id), sample_id TEXT NOT NULL REFERENCES samples(id), PRIMARY KEY(event_id, sample_id));
CREATE TABLE IF NOT EXISTS event_outputs (event_id TEXT NOT NULL REFERENCES process_events(id), sample_id TEXT NOT NULL REFERENCES samples(id), PRIMARY KEY(event_id, sample_id));
CREATE TABLE IF NOT EXISTS sample_usages (
  event_id TEXT NOT NULL REFERENCES process_events(id),
  sample_id TEXT NOT NULL REFERENCES samples(id),
  usage_type TEXT NOT NULL CHECK(usage_type IN ('consumed','non_destructive','aliquot')),
  created_at TEXT NOT NULL,
  PRIMARY KEY(event_id, sample_id)
);
CREATE UNIQUE INDEX IF NOT EXISTS sample_single_destructive_usage ON sample_usages(sample_id) WHERE usage_type='consumed';
CREATE TABLE IF NOT EXISTS entity_changes (id TEXT PRIMARY KEY, entity_type TEXT NOT NULL, entity_id TEXT NOT NULL, field_path TEXT NOT NULL, old_value_json TEXT NOT NULL, new_value_json TEXT NOT NULL, actor_id TEXT NOT NULL DEFAULT 'local_user', changed_at TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS qpcr_plate_wells (id TEXT PRIMARY KEY, experiment_id TEXT NOT NULL REFERENCES experiments(id), source_cdna_sample_id TEXT NOT NULL REFERENCES samples(id), target_name TEXT NOT NULL, technical_replicate_index INTEGER NOT NULL, plate_position TEXT NOT NULL, created_at TEXT NOT NULL, UNIQUE(experiment_id, plate_position));
CREATE TABLE IF NOT EXISTS assay_items (
  id TEXT PRIMARY KEY,
  record_id TEXT NOT NULL REFERENCES records(id),
  display_name TEXT NOT NULL,
  position INTEGER NOT NULL,
  metadata_json TEXT NOT NULL DEFAULT '{}',
  UNIQUE(record_id, display_name),
  UNIQUE(record_id, position)
);
CREATE TABLE IF NOT EXISTS assay_plates (
  id TEXT PRIMARY KEY,
  record_id TEXT NOT NULL REFERENCES records(id),
  name TEXT NOT NULL,
  plate_model TEXT NOT NULL CHECK(plate_model IN ('6','12','24','48','96','384')),
  created_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS assay_well_mappings (
  id TEXT PRIMARY KEY,
  plate_id TEXT NOT NULL REFERENCES assay_plates(id),
  well_position TEXT NOT NULL,
  sample_id TEXT REFERENCES samples(id),
  assay_item_id TEXT REFERENCES assay_items(id),
  assignment_role TEXT NOT NULL DEFAULT 'measurement' CHECK(assignment_role IN ('measurement','blank','standard')),
  metadata_json TEXT NOT NULL DEFAULT '{}',
  created_at TEXT NOT NULL,
  UNIQUE(plate_id, well_position),
  CHECK(assignment_role <> 'measurement' OR (sample_id IS NOT NULL AND assay_item_id IS NOT NULL))
);
CREATE TABLE IF NOT EXISTS assay_raw_imports (
  id TEXT PRIMARY KEY,
  record_id TEXT NOT NULL REFERENCES records(id),
  plate_id TEXT NOT NULL REFERENCES assay_plates(id),
  attachment_id TEXT NOT NULL UNIQUE REFERENCES attachments(id),
  metric_key TEXT NOT NULL,
  well_column TEXT NOT NULL,
  measurement_column TEXT NOT NULL,
  content_sha256 TEXT NOT NULL,
  imported_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS assay_raw_measurements (
  id TEXT PRIMARY KEY,
  import_id TEXT NOT NULL REFERENCES assay_raw_imports(id),
  well_position TEXT NOT NULL,
  metric_key TEXT NOT NULL,
  numeric_value REAL,
  text_value TEXT NOT NULL,
  raw_row_json TEXT NOT NULL,
  UNIQUE(import_id, well_position, metric_key)
);
CREATE TABLE IF NOT EXISTS qpcr_delta_ct_analyses (
  id TEXT PRIMARY KEY,
  record_id TEXT NOT NULL REFERENCES records(id),
  name TEXT NOT NULL,
  config_json TEXT NOT NULL,
  result_json TEXT NOT NULL,
  created_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS qpcr_delta_delta_ct_analyses (
  id TEXT PRIMARY KEY,
  record_id TEXT NOT NULL REFERENCES records(id),
  delta_ct_analysis_id TEXT NOT NULL REFERENCES qpcr_delta_ct_analyses(id),
  name TEXT NOT NULL,
  config_json TEXT NOT NULL,
  result_json TEXT NOT NULL,
  created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS assay_items_record_index ON assay_items(record_id);
CREATE INDEX IF NOT EXISTS assay_plates_record_index ON assay_plates(record_id);
CREATE INDEX IF NOT EXISTS assay_mappings_plate_index ON assay_well_mappings(plate_id);
CREATE INDEX IF NOT EXISTS assay_raw_imports_plate_index ON assay_raw_imports(plate_id);
CREATE INDEX IF NOT EXISTS assay_raw_measurements_import_index ON assay_raw_measurements(import_id);
CREATE INDEX IF NOT EXISTS qpcr_delta_ct_record_index ON qpcr_delta_ct_analyses(record_id);
CREATE INDEX IF NOT EXISTS qpcr_delta_delta_ct_record_index ON qpcr_delta_delta_ct_analyses(record_id);
CREATE TABLE IF NOT EXISTS export_manifests (
  id TEXT PRIMARY KEY,
  date_from TEXT NOT NULL,
  date_to TEXT NOT NULL,
  record_ids_json TEXT NOT NULL,
  content_sha256 TEXT NOT NULL,
  relative_path TEXT NOT NULL CHECK(relative_path NOT LIKE '/%'),
  status TEXT NOT NULL CHECK(status IN ('previewed','print_requested')),
  created_at TEXT NOT NULL
);
