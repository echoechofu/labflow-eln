import type { RecordItem, Sample, Task } from "./domain";

export type SampleSourceKind = "direct_parent" | "other_task" | "external";

export interface SampleSourceInfo {
  kind: SampleSourceKind;
  sourceTask?: Task;
}

export function eligibleParentTaskOptions(
  tasks: Task[],
  experimentId: string,
  currentTaskId: string,
  currentStart: string,
) {
  return tasks
    .filter(
      (candidate) =>
        candidate.experimentId === experimentId &&
        candidate.id !== currentTaskId &&
        candidate.start < currentStart,
    )
    .sort(
      (left, right) =>
        left.start.localeCompare(right.start) ||
        left.end.localeCompare(right.end) ||
        left.title.localeCompare(right.title),
    );
}

export function sampleSourceInfo(
  sample: Sample,
  currentTask: Task,
  tasks: Task[],
  records: RecordItem[],
): SampleSourceInfo {
  if (sample.origin === "external") return { kind: "external" };
  const sourceRecord = records.find((record) => record.id === sample.source);
  const sourceTask = tasks.find((task) => task.id === sourceRecord?.taskId);
  return {
    kind:
      sourceTask && currentTask.parentTaskIds?.includes(sourceTask.id)
        ? "direct_parent"
        : "other_task",
    sourceTask,
  };
}

export function groupSamplesBySource(
  samples: Sample[],
  currentTask: Task,
  tasks: Task[],
  records: RecordItem[],
) {
  const groups: Record<SampleSourceKind, Sample[]> = {
    direct_parent: [],
    other_task: [],
    external: [],
  };
  for (const sample of samples) {
    groups[sampleSourceInfo(sample, currentTask, tasks, records).kind].push(
      sample,
    );
  }
  for (const group of Object.values(groups)) {
    group.sort((left, right) => {
      const leftTask = sampleSourceInfo(
        left,
        currentTask,
        tasks,
        records,
      ).sourceTask;
      const rightTask = sampleSourceInfo(
        right,
        currentTask,
        tasks,
        records,
      ).sourceTask;
      return (
        (leftTask?.start || "9999").localeCompare(rightTask?.start || "9999") ||
        left.code.localeCompare(right.code)
      );
    });
  }
  return groups;
}
