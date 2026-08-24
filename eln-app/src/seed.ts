import type { Experiment, Protocol, RecordItem, Sample, Task } from "./domain";

export const experiments: Experiment[] = [
  {
    id: "exp-template",
    code: "EXP001",
    title: "A549 siRNA 筛选模板",
    description: "模板工作流：复苏 → 传代 → 铺板 → 刺激/成像 → RNA",
    color: "#167c80",
  },
];

export const tasks: Task[] = [
  {
    id: "task-template-thaw",
    experimentId: "exp-template",
    title: "细胞复苏",
    start: "2026-08-24T08:00:00",
    end: "2026-08-24T09:00:00",
    status: "completed",
    recordId: "record-template-thaw",
    parentTaskIds: [],
  },
  {
    id: "task-template-passage",
    experimentId: "exp-template",
    title: "细胞传代",
    start: "2026-08-25T09:00:00",
    end: "2026-08-25T10:00:00",
    status: "completed",
    recordId: "record-template-passage",
    parentTaskIds: ["task-template-thaw"],
  },
  {
    id: "task-template-plating",
    experimentId: "exp-template",
    title: "铺 6 孔板",
    start: "2026-08-26T09:00:00",
    end: "2026-08-26T10:00:00",
    status: "completed",
    recordId: "record-template-plating",
    parentTaskIds: ["task-template-passage"],
  },
  {
    id: "task-template-treatment",
    experimentId: "exp-template",
    title: "siRNA 加刺激",
    start: "2026-08-27T09:00:00",
    end: "2026-08-27T10:00:00",
    status: "completed",
    recordId: "record-template-treatment",
    parentTaskIds: ["task-template-plating"],
  },
  {
    id: "task-template-imaging",
    experimentId: "exp-template",
    title: "细胞成像",
    start: "2026-08-27T15:00:00",
    end: "2026-08-27T16:00:00",
    status: "planned",
    parentTaskIds: ["task-template-plating"],
  },
  {
    id: "task-template-rna",
    experimentId: "exp-template",
    title: "提取 RNA",
    start: "2026-08-28T09:00:00",
    end: "2026-08-28T10:30:00",
    status: "planned",
    parentTaskIds: ["task-template-treatment", "task-template-imaging"],
  },
];

export const protocols: Protocol[] = [
  {
    id: "pro-cell-thaw",
    name: "细胞复苏",
    category: "细胞培养",
    version: 2,
    blocks: ["37℃ 水浴复苏", "离心", "重悬培养"],
    accent: "#167c80",
  },
  {
    id: "pro-cell-passage",
    name: "细胞传代",
    category: "细胞培养",
    version: 2,
    blocks: ["选择细胞", "传代", "记录培养方式"],
    accent: "#167c80",
  },
  {
    id: "pro-cell-plating",
    name: "细胞铺板",
    category: "细胞培养",
    version: 3,
    blocks: ["选择细胞", "器皿与规格", "铺板"],
    accent: "#167c80",
  },
  {
    id: "pro-cell-treatment",
    name: "细胞加刺激",
    category: "细胞培养",
    version: 3,
    blocks: ["选择孔板", "刺激分组", "输出孔样本"],
    accent: "#167c80",
  },
  {
    id: "pro-rna",
    name: "RNA Extraction — Trizol",
    category: "分子生物学",
    version: 1,
    blocks: ["选择上级 Task", "选择输入 Sample", "RNA 提取", "输出 RNA"],
    accent: "#6957e8",
  },
];

export const samples: Sample[] = [
  {
    id: "sample-template-cell01",
    experimentId: "exp-template",
    code: "EXP001-CELL01",
    type: "CELL",
    displayName: "A549 复苏细胞",
  },
  {
    id: "sample-template-cell02",
    experimentId: "exp-template",
    code: "EXP001-CELL02",
    type: "CELL",
    displayName: "A549 传代细胞",
    parent: "sample-template-cell01",
  },
  {
    id: "sample-template-plate01",
    experimentId: "exp-template",
    code: "EXP001-PLATE01",
    type: "PLATE",
    displayName: "A549 6孔板",
    parent: "sample-template-cell02",
    metadata: { plate_capacity: 6 },
  },
  ...["A01", "A02", "A03", "B01", "B02", "B03"].map((well, index): Sample => ({
    id: `sample-template-well0${index + 1}`,
    experimentId: "exp-template",
    code: `EXP001-WELL0${index + 1}`,
    type: "WELL",
    displayName: well,
    parent: "sample-template-plate01",
    metadata: {
      well_position: well,
      treatment_factor: index < 3 ? "si NC" : "si 123",
      treatment_duration: "24h",
    },
  })),
];

export const records: RecordItem[] = [];
