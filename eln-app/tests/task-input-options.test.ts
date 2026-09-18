import assert from "node:assert/strict";
import test from "node:test";
import type { RecordItem, Sample, Task } from "../src/domain.ts";
import {
  eligibleRecordInputSamples,
  eligibleParentTaskOptions,
  groupSamplesBySource,
  sampleSourceInfo,
} from "../src/taskInputs.ts";

const task = (id: string, start: string, experimentId = "exp"): Task => ({
  id,
  experimentId,
  title: id,
  start,
  end: start.replace(":00", ":30"),
  status: "planned",
});

test("parent Task options only include earlier Tasks and are chronological", () => {
  const current = task("current", "2026-08-26T12:00");
  const options = eligibleParentTaskOptions(
    [
      task("later", "2026-08-26T13:00"),
      task("early-b", "2026-08-26T10:00"),
      current,
      task("same", "2026-08-26T12:00"),
      task("early-a", "2026-08-26T09:00"),
      task("other-experiment", "2026-08-26T08:00", "other"),
    ],
    "exp",
    current.id,
    current.start,
  );
  assert.deepEqual(
    options.map((item) => item.id),
    ["early-a", "early-b"],
  );
});

test("Sample source groups distinguish direct parent, other Task, and external", () => {
  const parent = {
    ...task("parent", "2026-08-25T09:00"),
    recordId: "record-parent",
  };
  const other = {
    ...task("other", "2026-08-24T09:00"),
    recordId: "record-other",
  };
  const current = {
    ...task("current", "2026-08-26T09:00"),
    parentTaskIds: [parent.id],
  };
  const records = [
    { id: "record-parent", taskId: parent.id, outputs: ["direct"] },
    { id: "record-other", taskId: other.id, outputs: ["other"] },
  ] as RecordItem[];
  const samples = [
    { id: "direct", code: "EXP-RNA02", type: "RNA", source: "record-parent" },
    { id: "other", code: "EXP-RNA01", type: "RNA", source: "record-other" },
    { id: "external", code: "EXP-RNA03", type: "RNA", origin: "external" },
  ] as Sample[];
  assert.equal(
    sampleSourceInfo(samples[0], current, [parent, other], records).kind,
    "direct_parent",
  );
  const groups = groupSamplesBySource(
    samples,
    current,
    [parent, other],
    records,
  );
  assert.deepEqual(
    groups.direct_parent.map((sample) => sample.id),
    ["direct"],
  );
  assert.deepEqual(
    groups.other_task.map((sample) => sample.id),
    ["other"],
  );
  assert.deepEqual(
    groups.external.map((sample) => sample.id),
    ["external"],
  );
});

test("a Sample passed through by a parent Record is a direct-parent output", () => {
  const creator = task("creator", "2026-08-24T09:00");
  const parent = task("parent", "2026-08-25T09:00");
  const current = {
    ...task("current", "2026-08-26T09:00"),
    parentTaskIds: [parent.id],
  };
  const records = [
    {
      id: "record-creator",
      taskId: creator.id,
      outputs: ["continued-sample"],
    },
    {
      id: "record-parent",
      taskId: parent.id,
      inputs: ["continued-sample"],
      outputs: ["continued-sample"],
    },
  ] as RecordItem[];
  const sample = {
    id: "continued-sample",
    code: "EXP-CELL01",
    type: "CELL",
    source: "record-creator",
  } as Sample;

  assert.deepEqual(
    sampleSourceInfo(sample, current, [creator, parent], records),
    { kind: "direct_parent", sourceTask: parent },
  );
});

test("outputs from multiple parent Tasks all belong to the direct-parent group", () => {
  const parentA = task("parent-a", "2026-08-25T09:00");
  const parentB = task("parent-b", "2026-08-25T10:00");
  const current = {
    ...task("current", "2026-08-26T09:00"),
    parentTaskIds: [parentA.id, parentB.id],
  };
  const records = [
    { id: "record-a", taskId: parentA.id, outputs: ["sample-a"] },
    { id: "record-b", taskId: parentB.id, outputs: ["sample-b"] },
  ] as RecordItem[];
  const samples = [
    { id: "sample-a", code: "EXP-RNA01", type: "RNA" },
    { id: "sample-b", code: "EXP-RNA02", type: "RNA" },
  ] as Sample[];

  const groups = groupSamplesBySource(
    samples,
    current,
    [parentA, parentB],
    records,
  );
  assert.deepEqual(
    groups.direct_parent.map((sample) => sample.id),
    ["sample-a", "sample-b"],
  );
  assert.deepEqual(groups.other_task, []);
});

test("consumed Samples are excluded from Record input candidates", () => {
  const samples = [
    {
      id: "available",
      experimentId: "exp",
      code: "EXP-RNA01",
      type: "rna",
    },
    {
      id: "consumed",
      experimentId: "exp",
      code: "EXP-RNA02",
      type: "RNA",
      consumed: true,
    },
    {
      id: "wrong-experiment",
      experimentId: "other",
      code: "OTHER-RNA01",
      type: "RNA",
    },
    {
      id: "wrong-type",
      experimentId: "exp",
      code: "EXP-CELL01",
      type: "CELL",
    },
  ] as Sample[];

  assert.deepEqual(
    eligibleRecordInputSamples(samples, "exp", ["RNA"]).map(
      (sample) => sample.id,
    ),
    ["available"],
  );
});

test("an unrestricted Protocol accepts every available Sample type", () => {
  const samples = [
    { id: "cell", experimentId: "exp", code: "CELL-1", type: "CELL" },
    { id: "rna", experimentId: "exp", code: "RNA-1", type: "RNA" },
    {
      id: "consumed",
      experimentId: "exp",
      code: "RNA-2",
      type: "RNA",
      consumed: true,
    },
  ] as Sample[];
  assert.deepEqual(
    eligibleRecordInputSamples(samples, "exp", []).map((sample) => sample.id),
    ["cell", "rna"],
  );
});
