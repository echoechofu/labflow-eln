import express from "express";
import { experiments, protocols, records, samples, tasks } from "./src/seed.ts";
import {
  appDataPaths,
  getLegacyDatabasePath,
  migrateLegacyDatabase,
} from "./server/appDataPath.ts";
import { openDatabase, validateDatabase } from "./server/database.ts";

const paths = appDataPaths();
migrateLegacyDatabase(
  paths,
  getLegacyDatabasePath(process.cwd()),
  validateDatabase,
);
paths.ensureUserDataDirectories();
const databasePath = paths.getDatabasePath();
const db = openDatabase(databasePath);
type Store = {
  experiments: typeof experiments;
  tasks: typeof tasks;
  protocols: typeof protocols;
  samples: typeof samples;
  records: typeof records;
};
const seed = () => {
  if (
    (
      db.prepare("SELECT count(*) as count FROM experiments").get() as {
        count: number;
      }
    ).count
  )
    return;
  writeStore({ experiments, tasks, protocols, samples, records });
};
const writeStore = db.transaction((store: Store) => {
  for (const table of [
    "record_changes",
    "record_samples",
    "sample_relations",
    "samples",
    "records",
    "protocol_versions",
    "protocols",
    "tasks",
    "experiments",
  ])
    db.prepare(`DELETE FROM ${table}`).run();
  for (const e of store.experiments)
    db.prepare("INSERT INTO experiments VALUES (?,?,?,?,?)").run(
      e.id,
      e.code,
      e.title,
      e.description,
      e.color,
    );
  for (const p of store.protocols) {
    db.prepare("INSERT INTO protocols VALUES (?,?,?,?,?)").run(
      p.id,
      p.name,
      p.category,
      p.version,
      p.accent,
    );
    db.prepare("INSERT INTO protocol_versions VALUES (?,?,?)").run(
      p.id,
      p.version,
      JSON.stringify({ blocks: p.blocks }),
    );
  }
  for (const t of store.tasks)
    db.prepare("INSERT INTO tasks VALUES (?,?,?,?,?,?,?)").run(
      t.id,
      t.experimentId,
      t.title,
      t.start,
      t.end,
      t.status,
      t.recordId ?? null,
    );
  for (const r of store.records) {
    db.prepare("INSERT INTO records VALUES (?,?,?,?,?,?,?)").run(
      r.id,
      r.taskId,
      r.experimentId,
      r.protocolId,
      JSON.stringify(store.protocols.find((p) => p.id === r.protocolId) ?? {}),
      JSON.stringify({
        notes: r.notes,
        inputs: r.inputs,
        outputs: r.outputs,
        title: r.title,
      }),
      r.updated,
    );
    for (const h of r.history)
      db.prepare("INSERT INTO record_changes VALUES (?,?,?,?,?,?,?)").run(
        h.id,
        r.id,
        h.field,
        JSON.stringify(h.from),
        JSON.stringify(h.to),
        "local_user",
        h.at,
      );
  }
  for (const s of store.samples) {
    db.prepare("INSERT INTO samples VALUES (?,?,?,?,?,?,?)").run(
      s.id,
      "local",
      store.records.find((r) => r.id === s.source)?.experimentId ?? "exp-23",
      s.code,
      s.type,
      s.source ?? null,
      s.parent ?? null,
    );
    if (s.parent)
      db.prepare("INSERT INTO sample_relations VALUES (?,?,?,?)").run(
        `rel-${s.parent}-${s.id}`,
        s.parent,
        s.id,
        "derived_from",
      );
  }
  for (const r of store.records) {
    for (const id of r.inputs)
      db.prepare("INSERT INTO record_samples VALUES (?,?,?)").run(
        r.id,
        id,
        "input",
      );
    for (const id of r.outputs)
      db.prepare("INSERT INTO record_samples VALUES (?,?,?)").run(
        r.id,
        id,
        "output",
      );
  }
});
const readStore = (): Store => {
  const ps = db.prepare("SELECT * FROM protocols").all() as Array<{
    id: string;
    name: string;
    category: string;
    active_version: number;
    accent: string;
  }>;
  const rs = db.prepare("SELECT * FROM records").all() as Array<{
    id: string;
    task_id: string;
    experiment_id: string;
    protocol_id: string;
    current_data_json: string;
    updated_at: string;
  }>;
  return {
    experiments: (
      db.prepare("SELECT * FROM experiments").all() as Array<{
        id: string;
        experiment_code: string;
        title: string;
        description: string;
        color: string;
      }>
    ).map((x) => ({
      id: x.id,
      code: x.experiment_code,
      title: x.title,
      description: x.description,
      color: x.color,
    })),
    tasks: (
      db.prepare("SELECT * FROM tasks").all() as Array<{
        id: string;
        experiment_id: string;
        title: string;
        start_time: string;
        end_time: string;
        status: "planned" | "in_progress" | "completed";
        record_id?: string;
      }>
    ).map((x) => ({
      id: x.id,
      experimentId: x.experiment_id,
      title: x.title,
      start: x.start_time,
      end: x.end_time,
      status: x.status,
      recordId: x.record_id,
      parentTaskIds: (
        db
          .prepare(
            "SELECT parent_task_id FROM task_relations WHERE child_task_id=? ORDER BY created_at,id",
          )
          .all(x.id) as Array<{ parent_task_id: string }>
      ).map((relation) => relation.parent_task_id),
    })),
    protocols: ps.map((p) => ({
      id: p.id,
      name: p.name,
      category: p.category,
      version: p.active_version,
      accent: p.accent,
      blocks: (
        JSON.parse(
          (
            db
              .prepare(
                "SELECT schema_json FROM protocol_versions WHERE protocol_id=? AND version_number=?",
              )
              .get(p.id, p.active_version) as { schema_json: string }
          ).schema_json,
        ) as { blocks: string[] }
      ).blocks,
    })),
    samples: (
      db.prepare("SELECT * FROM samples").all() as Array<{
        id: string;
        sample_code: string;
        sample_type: string;
        source_record_id?: string;
        parent_sample_id?: string;
      }>
    ).map((x) => ({
      id: x.id,
      code: x.sample_code,
      type: x.sample_type,
      source: x.source_record_id,
      parent: x.parent_sample_id,
    })),
    records: rs.map((r) => {
      const data = JSON.parse(r.current_data_json) as {
        notes: string;
        inputs: string[];
        outputs: string[];
        title: string;
      };
      return {
        id: r.id,
        taskId: r.task_id,
        experimentId: r.experiment_id,
        protocolId: r.protocol_id,
        title: data.title,
        updated: r.updated_at,
        notes: data.notes,
        inputs: data.inputs,
        outputs: data.outputs,
        history: (
          db
            .prepare("SELECT * FROM record_changes WHERE record_id=?")
            .all(r.id) as Array<{
            id: string;
            field_path: string;
            old_value_json: string;
            new_value_json: string;
            changed_at: string;
          }>
        ).map((h) => ({
          id: h.id,
          field: h.field_path,
          from: JSON.parse(h.old_value_json) as string,
          to: JSON.parse(h.new_value_json) as string,
          at: h.changed_at,
        })),
      };
    }),
  };
};
seed();
const app = express();
app.use(express.json({ limit: "2mb" }));
app.get("/api/store", (_, res) => res.json(readStore()));
app.put("/api/store", (req, res) => {
  writeStore(req.body as Store);
  res.status(204).end();
});
app.get("/api/health", (_, res) =>
  res.json({
    database: "ok",
    location: "user-data",
    attachmentsDirectory: paths.getAttachmentsDir(),
  }),
);
app.listen(8787, () =>
  console.log(`Labflow SQLite API ready: ${databasePath}`),
);
