export type TaskStatus = "planned" | "in_progress" | "completed";
export type NavPage =
  "calendar" | "experiments" | "protocols" | "records" | "data";

export interface Experiment {
  id: string;
  code: string;
  title: string;
  description: string;
  color: string;
}
export interface Task {
  id: string;
  experimentId: string;
  title: string;
  start: string;
  end: string;
  status: TaskStatus;
  recordId?: string;
  parentTaskIds?: string[];
}
export interface ProtocolField {
  key: string;
  label: string;
  kind: "text" | "select" | "number" | "samples" | "plate_layout";
  required?: boolean;
  options?: string[];
  visibleWhen?: { key: string; value: string };
  visibleForInputTypes?: string[];
  defaultValue?: string;
}
export interface ProtocolExecution {
  engine?: "sample_flow_v1";
  eventType: string;
  inputTypes?: string[];
  inputSource?: "parent_task_outputs" | "experiment_samples";
  inputCardinality?: "one" | "many";
  outputType?: string;
  outputMode:
    | "one"
    | "count"
    | "per_input"
    | "per_input_count"
    | "same_sample"
    | "plate_or_dish"
    | "plate_wells"
    | "none";
  resultTypes?: string[];
  consumptionPolicy?: "consume" | "non_destructive" | "aliquot";
}
export interface TerminalAssayDefinition {
  itemLabel: string;
  metricKey: string;
  metricLabel: string;
  plateModels: string[];
}
export interface Protocol {
  id: string;
  name: string;
  category: string;
  version: number;
  blocks: string[];
  accent: string;
  description?: string;
  origin?: "builtin" | "user";
  activeVersionOrigin?: "builtin" | "user";
  fields?: ProtocolField[];
  template?: string;
  templateSelector?: string;
  templateVariants?: Record<string, string>;
  execution?: ProtocolExecution;
  terminalAssay?: TerminalAssayDefinition;
}
export interface SampleTypeDefinition {
  canonicalType: string;
  displayName: string;
  origin: "builtin" | "user";
}
export interface Sample {
  id: string;
  experimentId?: string;
  code: string;
  type: string;
  source?: string;
  parent?: string;
  displayName?: string;
  metadata?: Record<string, unknown>;
  lineageStatus?: "complete" | "partial" | "unknown";
  consumed?: boolean;
  origin?: "internal" | "external";
}

export const normalizeSampleType = (value: string) => value.toUpperCase();

export const sampleTypeLabel = (value: string) => {
  const canonical = normalizeSampleType(value);
  return canonical === "CDNA" ? "cDNA" : canonical;
};
export interface RecordItem {
  id: string;
  taskId: string;
  experimentId: string;
  protocolId: string;
  protocolName?: string;
  protocolSnapshot?: {
    name?: string;
    version?: number;
    schema?: {
      terminalAssay?: TerminalAssayDefinition;
      [key: string]: unknown;
    };
  };
  title: string;
  updated: string;
  notes: string;
  inputs: string[];
  outputs: string[];
  history: HistoryItem[];
  renderedContent?: string;
  analysisSections?: {
    id: string;
    kind: "delta_ct" | "delta_delta_ct";
    title: string;
    text: string;
    savedAt: string;
  }[];
  protocolVersion?: number;
  attachments?: {
    id: string;
    fileName: string;
    relativePath: string;
    mimeType?: string;
    size?: number;
  }[];
  values?: Record<string, string>;
  results?: {
    id: string;
    type: string;
    data: Record<string, unknown>;
  }[];
}
export interface HistoryItem {
  id: string;
  field: string;
  from: string;
  to: string;
  at: string;
}

export const statusLabel: Record<TaskStatus, string> = {
  planned: "计划中",
  in_progress: "进行中",
  completed: "已完成",
};

export function formatTime(value: string) {
  return new Date(value).toLocaleTimeString("zh-CN", {
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
  });
}
export function dayLabel(value: string) {
  return new Intl.DateTimeFormat("zh-CN", {
    weekday: "short",
    month: "numeric",
    day: "numeric",
  }).format(new Date(value));
}
