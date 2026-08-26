import assert from "node:assert/strict";
import test from "node:test";
import type { RecordItem, Sample, Task } from "../src/domain.ts";
import {
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
    { id: "record-parent", taskId: parent.id },
    { id: "record-other", taskId: other.id },
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
