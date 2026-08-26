import {
  experiments as seedExperiments,
  protocols as seedProtocols,
  records as seedRecords,
  samples as seedSamples,
  tasks as seedTasks,
} from "./seed";
import type {
  Experiment,
  Protocol,
  RecordItem,
  Sample,
  SampleTypeDefinition,
  Task,
} from "./domain";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";

export interface Store {
  experiments: Experiment[];
  tasks: Task[];
  protocols: Protocol[];
  sampleTypes: SampleTypeDefinition[];
  samples: Sample[];
  records: RecordItem[];
}
export interface WorkspaceBackupSummary {
  appVersion: string;
  exportedAt: string;
  databaseSchemaVersion: number;
  counts: {
    experiments: number;
    tasks: number;
    records: number;
    samples: number;
    attachments: number;
    files: number;
  };
}
export interface WorkspaceBackupExport {
  path: string;
  summary: WorkspaceBackupSummary;
}
export interface WorkspaceBackupRestore {
  recoveryBackupPath: string;
  summary: WorkspaceBackupSummary;
}

const backupFileName = () => {
  const stamp = new Date().toISOString().replace(/[:.]/g, "-");
  return `LabFlow-Backup-${stamp}.labflow-backup`;
};

export async function exportWorkspaceBackup() {
  desktopOnly();
  const destination = await save({
    title: "导出 LabFlow 工作区备份",
    defaultPath: backupFileName(),
    filters: [{ name: "LabFlow Backup", extensions: ["labflow-backup"] }],
  });
  if (!destination) return undefined;
  return invoke<WorkspaceBackupExport>("export_workspace_backup", {
    destination,
    exportedAt: new Date().toISOString(),
  });
}

export async function chooseWorkspaceBackup() {
  desktopOnly();
  const selected = await open({
    title: "选择 LabFlow 工作区备份",
    multiple: false,
    directory: false,
    filters: [{ name: "LabFlow Backup", extensions: ["labflow-backup"] }],
  });
  if (!selected || Array.isArray(selected)) return undefined;
  const summary = await invoke<WorkspaceBackupSummary>(
    "inspect_workspace_backup",
    { path: selected },
  );
  return { path: selected, summary };
}

export async function restoreWorkspaceBackup(path: string) {
  desktopOnly();
  return invoke<WorkspaceBackupRestore>("restore_workspace_backup", {
    path,
    importedAt: new Date().toISOString(),
  });
}
const clone = <T>(value: T): T => JSON.parse(JSON.stringify(value)) as T;
export async function loadStore(): Promise<Store> {
  if (isTauri()) return invoke<Store>("get_store");
  const response = await fetch("/api/store");
  if (!response.ok) throw new Error("Cannot load local SQLite data");
  return response.json() as Promise<Store>;
}
export function initialStore(): Store {
  return clone({
    experiments: seedExperiments,
    tasks: seedTasks,
    protocols: seedProtocols,
    sampleTypes: [
      "CELL",
      "PLATE",
      "DISH",
      "WELL",
      "RNA",
      "CDNA",
      "PROTEIN",
      "SUP",
    ].map((canonicalType) => ({
      canonicalType,
      displayName: canonicalType === "CDNA" ? "cDNA" : canonicalType,
      origin: "builtin" as const,
    })),
    samples: seedSamples,
    records: seedRecords,
  });
}
export async function saveStore(store: Store) {
  if (isTauri()) {
    await invoke("save_store", { store });
    return;
  }
  const response = await fetch("/api/store", {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(store),
  });
  if (!response.ok) throw new Error("Cannot save local SQLite data");
}
export type LineageSampleType =
  | "FROZEN_STOCK"
  | "CULTURE"
  | "CELL_PLATE_WELL"
  | "LYSATE"
  | "RNA"
  | "CDNA"
  | "OTHER";
export type LineageStatus = "complete" | "partial" | "unknown";
export interface ProcessEventInput {
  id: string;
  experimentId: string;
  eventType: string;
  occurredAt: string;
  parameters?: Record<string, unknown>;
  provenance?: "labflow_recorded" | "user_imported";
  recordId?: string;
}
export interface LineageOutput {
  id: string;
  code: string;
  displayName: string;
  sampleType: LineageSampleType;
  lineageStatus?: LineageStatus;
  metadata?: Record<string, unknown>;
}
const desktopOnly = () => {
  if (!isTauri())
    throw new Error("Lineage editing is available in the LabFlow desktop app.");
};
export async function createProcessEvent(
  event: ProcessEventInput,
  inputIds: string[],
  outputs: LineageOutput[],
) {
  desktopOnly();
  await invoke("create_process_event", { event, inputIds, outputs });
}
export async function applyTreatmentEvent(
  event: ProcessEventInput,
  sampleIds: string[],
) {
  desktopOnly();
  await invoke("apply_treatment_event", { event, sampleIds });
}
export async function getSampleDetail(sampleId: string) {
  desktopOnly();
  return invoke<SampleDetail>("sample_detail", { sampleId });
}
export interface SampleDetail {
  sample: {
    id: string;
    experimentId: string;
    code: string;
    displayName?: string;
    sampleType: string;
    lineageStatus?: string;
    metadata: Record<string, unknown>;
    createdAt?: string;
  };
  aliases: {
    id: string;
    alias: string;
    aliasType: string;
    createdAt: string;
  }[];
  upstream: string[];
  treatments: {
    eventId: string;
    occurredAt: string;
    parameters: Record<string, unknown>;
    provenance: string;
  }[];
}
export async function addSampleAlias(alias: {
  id: string;
  sampleId: string;
  alias: string;
  aliasType?: string;
  createdAt: string;
}) {
  desktopOnly();
  await invoke("add_sample_alias", { alias });
}
export async function deleteSampleAlias(id: string) {
  desktopOnly();
  await invoke("delete_sample_alias", { id });
}
export async function saveExperiment(experiment: Experiment) {
  desktopOnly();
  await invoke("save_experiment", {
    experiment,
    changedAt: new Date().toISOString(),
  });
}
export async function deleteExperiment(id: string) {
  desktopOnly();
  await invoke("delete_experiment", { id });
}
export interface TaskDraft {
  id: string;
  title: string;
  experimentId?: string;
  newExperimentId?: string;
  newExperimentCode?: string;
  start: string;
  end: string;
  updatedAt: string;
}
export async function saveTask(
  task: TaskDraft,
  newExperimentName?: string,
  parentTaskIds: string[] = [],
) {
  desktopOnly();
  return invoke<Task>("save_task", { task, newExperimentName, parentTaskIds });
}
export async function deleteTask(id: string) {
  desktopOnly();
  await invoke("delete_task", { id });
}
export async function deleteRecord(id: string) {
  desktopOnly();
  await invoke("delete_record", { id });
}
export async function updateRecordBody(id: string, renderedContent: string) {
  desktopOnly();
  const changedAt = new Date().toISOString();
  await invoke("update_record_body", {
    id,
    renderedContent,
    changeId: uid("record-change"),
    changedAt,
  });
}
export async function updateTaskStatus(id: string, status: Task["status"]) {
  desktopOnly();
  return invoke<Task>("update_task_status", { id, status });
}
export async function startTaskRecord(
  taskId: string,
  protocolId: string,
  recordId: string,
  values: Record<string, string>,
  inputSampleIds: string[] = [],
  externalInputs: ExternalSampleDraft[] = [],
) {
  desktopOnly();
  return invoke<Task>("start_task_record", {
    taskId,
    protocolId,
    recordId,
    values,
    inputSampleIds,
    externalInputs,
  });
}

export interface ExternalSampleDraft {
  sampleType: string;
  displayName: string;
  metadata?: Record<string, unknown>;
}

export interface UserProtocolDraft {
  id: string;
  name: string;
  description: string;
  category: string;
  accent: string;
  inputType: string;
  inputTypeDisplayName: string;
  outputBehavior:
    "same_sample" | "derived_one" | "derived_multiple" | "measurement_only";
  outputType?: string;
  outputTypeDisplayName?: string;
  consumptionPolicy: "retain" | "consume";
  template: string;
  createdAt: string;
}

export async function saveUserProtocol(request: UserProtocolDraft) {
  desktopOnly();
  return invoke<{ id: string; version: number }>("save_user_protocol", {
    request,
  });
}

export async function saveProtocolTemplateVersion(request: {
  protocolId: string;
  template?: string;
  templateVariants?: Record<string, string>;
  createdAt: string;
}) {
  desktopOnly();
  return invoke<{ id: string; previousVersion: number; version: number }>(
    "save_protocol_template_version",
    { request },
  );
}

export interface AssayWorkspace {
  items: {
    id: string;
    displayName: string;
    position: number;
    metadata: Record<string, unknown>;
  }[];
  plates: {
    id: string;
    name: string;
    plateModel: string;
    createdAt: string;
  }[];
  mappings: {
    id: string;
    plateId: string;
    wellPosition: string;
    sampleId: string;
    assayItemId: string;
    assignmentRole: "measurement" | "blank" | "standard";
    metadata: Record<string, unknown>;
  }[];
  imports: {
    id: string;
    plateId: string;
    attachmentId: string;
    fileName: string;
    relativePath: string;
    metricKey: string;
    wellColumn: string;
    measurementColumn: string;
    contentSha256: string;
    importedAt: string;
    measurementCount: number;
  }[];
  joinedWells: {
    mappingId: string;
    measurementId: string;
    plateId: string;
    plateName: string;
    wellPosition: string;
    sampleId: string;
    sampleCode: string;
    assayItemId: string;
    assayItem: string;
    importId: string;
    fileName: string;
    metricKey: string;
    numericValue?: number;
    textValue: string;
  }[];
  deltaCtAnalyses: QpcrDeltaCtAnalysis[];
  deltaDeltaCtAnalyses: QpcrDeltaDeltaCtAnalysis[];
}

export interface QpcrDeltaCtSampleResult {
  sampleId: string;
  sampleCode: string;
  targetMeanCq: number;
  referenceMeanCq: number;
  targetReplicateCount: number;
  referenceReplicateCount: number;
  deltaCt: number;
}

export interface QpcrDeltaCtAnalysis {
  id: string;
  recordId: string;
  name: string;
  config: {
    targetItemIds: string[];
    referenceItemIds: string[];
    includedMeasurementIds: string[];
    qcNotes: Record<string, string>;
  };
  result: {
    combinations: {
      targetItemId: string;
      referenceItemId: string;
      samples: QpcrDeltaCtSampleResult[];
    }[];
  };
  createdAt: string;
}

export interface QpcrDeltaDeltaCtAnalysis {
  id: string;
  recordId: string;
  deltaCtAnalysisId: string;
  name: string;
  config: {
    referenceItemId: string;
    controlMode: "shared" | "matched";
    sampleGroups: Record<string, string>;
    sharedControlGroup: string;
    controlRelations: Record<string, string>;
  };
  result: {
    combinations: {
      targetItemId: string;
      referenceItemId: string;
      samples: (QpcrDeltaCtSampleResult & {
        group: string;
        controlGroup: string;
        controlMeanDeltaCt: number;
        deltaDeltaCt: number;
        relativeExpression: number;
      })[];
    }[];
  };
  createdAt: string;
}

export async function getAssayWorkspace(recordId: string) {
  desktopOnly();
  return invoke<AssayWorkspace>("get_assay_workspace", { recordId });
}

export async function createAssayPlate(request: {
  id: string;
  recordId: string;
  name: string;
  plateModel: string;
  createdAt: string;
}) {
  desktopOnly();
  await invoke("create_assay_plate", { request });
}

export async function deleteEmptyAssayPlate(plateId: string) {
  desktopOnly();
  await invoke("delete_empty_assay_plate", { plateId });
}

export async function replaceAssayPlateMappings(
  plateId: string,
  mappings: {
    id: string;
    wellPosition: string;
    sampleId: string;
    assayItemId: string;
  }[],
) {
  desktopOnly();
  await invoke("replace_assay_plate_mappings", {
    plateId,
    mappings,
    changedAt: new Date().toISOString(),
  });
}

export interface RawUploadResult {
  id: string;
  attachmentId: string;
  relativePath: string;
  contentSha256: string;
  measurementCount: number;
}

export async function uploadAssayRawFile(
  request: {
    id: string;
    recordId: string;
    plateId: string;
    attachmentId: string;
    fileName: string;
    mimeType: string;
    metricKey: string;
    wellColumn: string;
    measurementColumn: string;
    importedAt: string;
  },
  bytes: number[],
) {
  desktopOnly();
  return invoke<RawUploadResult>("upload_assay_raw_file", { request, bytes });
}

export async function createQpcrDeltaCtAnalysis(request: {
  id: string;
  recordId: string;
  name: string;
  targetItemIds: string[];
  referenceItemIds: string[];
  includedMeasurementIds: string[];
  qcNotes: Record<string, string>;
  createdAt: string;
}) {
  desktopOnly();
  return invoke<QpcrDeltaCtAnalysis>("create_qpcr_delta_ct_analysis", {
    request,
  });
}

export async function createQpcrDeltaDeltaCtAnalysis(request: {
  id: string;
  recordId: string;
  deltaCtAnalysisId: string;
  name: string;
  referenceItemId: string;
  controlMode: "shared" | "matched";
  sampleGroups: Record<string, string>;
  sharedControlGroup: string;
  controlRelations: Record<string, string>;
  createdAt: string;
}) {
  desktopOnly();
  return invoke<QpcrDeltaDeltaCtAnalysis>(
    "create_qpcr_delta_delta_ct_analysis",
    { request },
  );
}

export interface ExportManifestResult {
  id: string;
  contentSha256: string;
  relativePath: string;
  recordCount: number;
}

export async function createExportManifest(request: {
  id: string;
  dateFrom: string;
  dateTo: string;
  recordIds: string[];
  createdAt: string;
}) {
  desktopOnly();
  return invoke<ExportManifestResult>("create_export_manifest", { request });
}

export async function markExportPrintRequested(id: string) {
  desktopOnly();
  await invoke("mark_export_print_requested", { id });
}
export async function createTreatmentDefinition(treatment: {
  id: string;
  experimentId: string;
  shortCode: string;
  name: string;
  parameters?: Record<string, unknown>;
  createdAt: string;
}) {
  desktopOnly();
  await invoke("create_treatment_definition", { treatment });
}
export async function archiveTreatmentDefinition(id: string) {
  desktopOnly();
  await invoke("archive_treatment_definition", {
    id,
    archivedAt: new Date().toISOString(),
  });
}
export async function createContainer(container: {
  id: string;
  experimentId: string;
  containerType: string;
  name: string;
  metadata?: Record<string, unknown>;
  createdAt: string;
}) {
  desktopOnly();
  await invoke("create_container", { container });
}
export async function deleteContainer(id: string) {
  desktopOnly();
  await invoke("delete_container", { id });
}
export async function assignSampleLocation(location: {
  id: string;
  sampleId: string;
  containerId: string;
  position: string;
  validFrom: string;
}) {
  desktopOnly();
  await invoke("assign_sample_location", { location });
}
export async function createQpcrMapping(mapping: {
  id: string;
  experimentId: string;
  sampleId: string;
  targetName: string;
  technicalReplicateIndex: number;
  platePosition: string;
  createdAt: string;
}) {
  desktopOnly();
  await invoke("create_qpcr_mapping", { mapping });
}
export async function deleteQpcrMapping(id: string) {
  desktopOnly();
  await invoke("delete_qpcr_mapping", { id });
}
export async function deleteOrArchiveSample(id: string) {
  desktopOnly();
  return invoke<string>("delete_or_archive_sample", {
    id,
    archivedAt: new Date().toISOString(),
  });
}
export async function deleteOrArchiveProcessEvent(id: string) {
  desktopOnly();
  return invoke<string>("delete_or_archive_process_event", {
    id,
    archivedAt: new Date().toISOString(),
  });
}
export interface LineageWorkspace {
  samples: {
    id: string;
    code: string;
    displayName: string;
    sampleType: string;
    lineageStatus: string;
    archivedAt?: string;
  }[];
  treatments: {
    id: string;
    shortCode: string;
    name: string;
    parameters: Record<string, unknown>;
    archivedAt?: string;
  }[];
  containers: {
    id: string;
    containerType: string;
    name: string;
    metadata: Record<string, unknown>;
  }[];
  qpcrMappings: {
    id: string;
    sampleId: string;
    targetName: string;
    technicalReplicateIndex: number;
    platePosition: string;
  }[];
  events: {
    id: string;
    eventType: string;
    occurredAt: string;
    archivedAt?: string;
  }[];
}
export async function getLineageWorkspace(experimentId: string) {
  desktopOnly();
  return invoke<LineageWorkspace>("lineage_workspace", { experimentId });
}
export function uid(prefix: string) {
  return `${prefix}-${crypto.randomUUID()}`;
}
export function suggestSampleCode(
  experimentCode: string,
  current: Sample[],
  type: string,
  parent?: Sample,
) {
  const root = parent
    ? (parent.code.match(/-S\d{3}/)?.[0] ?? "-S001")
    : `-S${String(current.filter((s) => s.code.startsWith(experimentCode)).length + 1).padStart(3, "0")}`;
  const siblings = current.filter((s) =>
    s.code.startsWith(`${experimentCode}${root}-${type}`),
  ).length;
  return `${experimentCode}${root}-${type}${parent ? String(siblings + 1).padStart(2, "0") : ""}`;
}
