import assert from "node:assert/strict";
import test from "node:test";
import type { Task } from "../src/domain.ts";
import { buildTaskGraph } from "../src/taskGraph.ts";

const task = (id: string, parentTaskIds: string[] = []): Task => ({
  id,
  experimentId: "exp",
  title: id,
  start: `2026-08-${String(20 + id.charCodeAt(0) - 96).padStart(2, "0")}T09:00:00`,
  end: "2026-08-30T10:00:00",
  status: "planned",
  parentTaskIds,
});

test("task graph lays out branches and merges as a DAG", () => {
  const layout = buildTaskGraph([
    task("a"),
    task("b", ["a"]),
    task("c", ["a"]),
    task("d", ["b", "c"]),
  ]);
  const levels = Object.fromEntries(
    layout.nodes.map((node) => [node.task.id, node.level]),
  );
  assert.deepEqual(levels, { a: 0, b: 1, c: 1, d: 2 });
  assert.equal(layout.edges.length, 4);
  assert.equal(layout.hasCycle, false);
});

test("task graph keeps isolated tasks visible", () => {
  const layout = buildTaskGraph([task("a"), task("b")]);
  assert.equal(layout.nodes.length, 2);
  assert.ok(layout.nodes.every((node) => !node.connected));
  assert.equal(layout.edges.length, 0);
});

test("task graph ignores invalid relations and safely surfaces cycles", () => {
  const invalid = buildTaskGraph([task("a", ["missing"])]);
  assert.equal(invalid.invalidRelationCount, 1);
  assert.equal(invalid.nodes.length, 1);

  const cyclic = buildTaskGraph([task("a", ["b"]), task("b", ["a"])]);
  assert.equal(cyclic.hasCycle, true);
  assert.equal(cyclic.nodes.length, 2);
  assert.equal(cyclic.edges.length, 2);
});
