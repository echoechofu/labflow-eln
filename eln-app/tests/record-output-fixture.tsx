// Browser-only QA harness: all data lives in memory and no persistence command is called.
import { createRoot } from "react-dom/client";
import { TaskDrawer } from "../src/RecordCreationDrawer";
import { initialStore } from "../src/repository";
import type { Protocol, RecordItem, Sample, Task } from "../src/domain";
import "../src/App.css";
import "../src/task-modal.css";

const store = initialStore();
const experiment = {
  id: "qa-experiment",
  code: "QA",
  title: "隔离交互验收",
  description: "",
  color: "#6957e8",
};
const parent: Task = {
  id: "qa-parent",
  experimentId: experiment.id,
  title: "动物准备",
  start: "2026-09-18T09:00:00",
  end: "2026-09-18T10:00:00",
  status: "completed",
  recordId: "qa-parent-record",
};
const task: Task = {
  id: "qa-task",
  experimentId: experiment.id,
  title: "组织采集",
  start: "2026-09-18T11:00:00",
  end: "2026-09-18T12:00:00",
  status: "planned",
  parentTaskIds: [parent.id],
};
const sample = {
  id: "qa-animal",
  experimentId: experiment.id,
  code: "QA-ANIMAL-001",
  type: "ANIMAL",
  displayName: "Mouse 01",
  source: "qa-parent-record",
  origin: "internal",
} as Sample;
const parentRecord = {
  id: "qa-parent-record",
  taskId: parent.id,
  experimentId: experiment.id,
  outputs: [sample.id],
} as RecordItem;
const protocol: Protocol = {
  id: "qa-harvest",
  name: "动物组织采集",
  category: "自定义",
  version: 1,
  blocks: [],
  accent: "#6957e8",
  description: "逐行登记多种输出材料",
  origin: "user",
  fields: [],
  template: "{{input_sample_summary}} → {{output_sample_summary}}",
  execution: {
    engine: "sample_flow_v1",
    eventType: "custom:qa-harvest",
    inputSource: "experiment_samples",
    inputCardinality: "many",
    inputTypes: ["ANIMAL"],
    inputTypePolicy: "uniform",
    outputMode: "record_many",
    consumptionPolicy: "non_destructive",
  },
};

Object.assign(window, {
  isTauri: true,
  __TAURI_INTERNALS__: {
    invoke: async (command: string) => {
      if (command === "start_task_record") return {};
      if (command === "save_protocol_template_version") return {};
      throw new Error(`QA fixture must not persist: ${command}`);
    },
  },
});

createRoot(document.getElementById("root")!).render(
  <TaskDrawer
    task={task}
    experiment={experiment}
    samples={[sample]}
    sampleTypes={store.sampleTypes}
    protocols={[protocol]}
    tasks={[parent, task]}
    records={[parentRecord]}
    close={() => {}}
    edit={() => {}}
    openRecord={() => {}}
    changed={() => {}}
    protocolsChanged={() => {}}
  />,
);
