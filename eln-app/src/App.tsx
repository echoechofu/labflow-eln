import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import "./App.css";
import "./task-modal.css";
import type { Experiment, NavPage, Protocol, Task } from "./domain";
import {
  dayLabel,
  formatTime,
  normalizeSampleType,
  sampleTypeLabel,
  statusLabel,
} from "./domain";
import {
  createExportManifest,
  beginRecordPdf,
  appendRecordPdfPage,
  finishRecordPdf,
  cancelRecordPdf,
  recordImagePreviewUrl,
  chooseRecordImage,
  chooseWorkspaceBackup,
  deleteProtocol,
  deleteRecord,
  deleteTask,
  loadStore,
  markExportPrintRequested,
  exportWorkspaceBackup,
  insertRecordImage,
  restoreWorkspaceBackup,
  saveTask,
  startTaskRecord,
  uid,
  updateRecordBody,
  updateTaskStatus,
  type ExternalSampleDraft,
  type Store,
  type WorkspaceBackupSummary,
} from "./repository";
import { RecordBody } from "./RecordBody";
import { recordPdfBlocks, renderRecordPdf } from "./recordPdf";
import {
  imageCaptionFromPath,
  insertImageReference,
  parseRecordBody,
} from "./recordBodyFormat";
import {
  buildTaskGraph,
  TASK_GRAPH_NODE_HEIGHT,
  TASK_GRAPH_NODE_WIDTH,
} from "./taskGraph";
import {
  eligibleParentTaskOptions,
  groupSamplesBySource,
  sampleSourceInfo,
  type SampleSourceKind,
} from "./taskInputs";
import { searchProtocols } from "./protocolSearch";
import TerminalAssayWorkspace from "./TerminalAssayWorkspace";
import {
  ProtocolCreationWizard,
  ProtocolTemplateEditor,
} from "./ProtocolEditor";

const HOURS = Array.from({ length: 24 }, (_, i) => i);
const HOUR_HEIGHT = 64;
const localDateTime = (d: Date) => {
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}T${p(d.getHours())}:${p(d.getMinutes())}`;
};
const addDays = (d: Date, n: number) => {
  const result = new Date(d);
  result.setDate(result.getDate() + n);
  return result;
};
const startOfWeek = (d: Date) => {
  const result = new Date(d);
  result.setDate(result.getDate() - (result.getDay() || 7) + 1);
  result.setHours(0, 0, 0, 0);
  return result;
};
const sameDate = (value: string, d: Date) => {
  const x = new Date(value);
  return (
    x.getFullYear() === d.getFullYear() &&
    x.getMonth() === d.getMonth() &&
    x.getDate() === d.getDate()
  );
};

type PlateTreatmentGroup = {
  factor: string;
  duration: string;
  wellCount: number;
};

const plateCapacity = (value: unknown) => {
  const text = String(value ?? "");
  const chineseCapacity = [
    ["三百八十四孔", 384],
    ["九十六孔", 96],
    ["四十八孔", 48],
    ["二十四孔", 24],
    ["十二孔", 12],
    ["六孔", 6],
  ].find(([label]) => text.includes(String(label)))?.[1];
  if (chineseCapacity) return Number(chineseCapacity);
  const match = text.match(/\d+/);
  const capacity = match ? Number(match[0]) : 0;
  return [6, 12, 24, 48, 96, 384].includes(capacity) ? capacity : 0;
};

function freshTask(): Task {
  const start = new Date();
  start.setMinutes(0, 0, 0);
  start.setHours(Math.max(8, start.getHours() + 1));
  const end = new Date(start);
  end.setHours(end.getHours() + 1);
  return {
    id: uid("task"),
    experimentId: "",
    title: "",
    start: localDateTime(start),
    end: localDateTime(end),
    status: "planned",
    parentTaskIds: [],
  };
}

export default function App() {
  const [store, setStore] = useState<Store>();
  const [page, setPage] = useState<NavPage>("calendar");
  const [selectedTask, setSelectedTask] = useState<Task>();
  const [taskForm, setTaskForm] = useState<Task>();
  const [openedRecordId, setOpenedRecordId] = useState<string>();
  const [week, setWeek] = useState(() => startOfWeek(new Date()));
  // Keep the week the user is viewing when the store refreshes.  Selecting
  // the first persisted task here made a successfully-created task appear to
  // vanish whenever that task belonged to a different week.
  const load = useCallback(() => void loadStore().then(setStore), []);
  useEffect(load, [load]);
  useEffect(() => {
    const refreshAfterExternalWrite = () => load();
    window.addEventListener("focus", refreshAfterExternalWrite);
    return () => window.removeEventListener("focus", refreshAfterExternalWrite);
  }, [load]);
  if (!store) return <main className="page">正在读取本地数据…</main>;
  const nav = [
    ["calendar", "◫", "日历"],
    ["experiments", "◈", "实验"],
    ["protocols", "▤", "Protocols"],
    ["records", "▧", "Records"],
    ["data", "⇅", "数据管理"],
  ] as const;
  return (
    <main className="app-shell" data-build-marker="task-crud-current">
      <aside className="sidebar">
        <div className="brand">
          <span>✦</span>LabFlow
        </div>
        <div className="workspace">
          <i>LF</i>
          <div>
            <b>我的实验室</b>
            <small>Local workspace</small>
          </div>
          <span>⌄</span>
        </div>
        <nav>
          {nav.map(([id, icon, label]) => (
            <button
              key={id}
              className={`nav ${page === id ? "active" : ""}`}
              onClick={() => setPage(id)}
            >
              <span>{icon}</span>
              {label}
            </button>
          ))}
        </nav>
      </aside>
      {page === "calendar" && (
        <Calendar
          store={store}
          week={week}
          setWeek={setWeek}
          openExisting={setSelectedTask}
          create={() => setTaskForm(freshTask())}
        />
      )}
      {page === "protocols" && (
        <ProtocolsPage
          protocols={store.protocols}
          sampleTypes={store.sampleTypes}
          records={store.records}
          changed={load}
        />
      )}
      {page === "experiments" && (
        <ExperimentsPage store={store} openTask={setSelectedTask} />
      )}
      {page === "records" && (
        <RecordsPage
          store={store}
          openedRecordId={openedRecordId}
          closeRecord={() => setOpenedRecordId(undefined)}
          openRecord={setOpenedRecordId}
          changed={load}
        />
      )}
      {page === "data" && <DataManagementPage changed={load} />}
      {selectedTask && (
        <TaskDrawer
          task={selectedTask}
          experiment={store.experiments.find(
            (item) => item.id === selectedTask.experimentId,
          )}
          samples={store.samples}
          sampleTypes={store.sampleTypes}
          protocols={store.protocols}
          tasks={store.tasks}
          records={store.records}
          close={() => setSelectedTask(undefined)}
          edit={() => {
            setTaskForm(selectedTask);
            setSelectedTask(undefined);
          }}
          openRecord={() => {
            setOpenedRecordId(selectedTask.recordId);
            setSelectedTask(undefined);
            setPage("records");
          }}
          changed={() => {
            setSelectedTask(undefined);
            load();
          }}
          protocolsChanged={load}
        />
      )}
      {taskForm && (
        <TaskModal
          task={taskForm}
          experiments={store.experiments}
          tasks={store.tasks}
          cancel={() => setTaskForm(undefined)}
          done={() => {
            setTaskForm(undefined);
            load();
          }}
        />
      )}
    </main>
  );
}

function BackupSummaryView({ summary }: { summary: WorkspaceBackupSummary }) {
  return (
    <dl className="backup-summary">
      <div>
        <dt>备份时间</dt>
        <dd>{new Date(summary.exportedAt).toLocaleString("zh-CN")}</dd>
      </div>
      <div>
        <dt>LabFlow 版本</dt>
        <dd>{summary.appVersion}</dd>
      </div>
      <div>
        <dt>Experiments</dt>
        <dd>{summary.counts.experiments}</dd>
      </div>
      <div>
        <dt>Tasks</dt>
        <dd>{summary.counts.tasks}</dd>
      </div>
      <div>
        <dt>Records</dt>
        <dd>{summary.counts.records}</dd>
      </div>
      <div>
        <dt>Samples</dt>
        <dd>{summary.counts.samples}</dd>
      </div>
      <div>
        <dt>附件 / 文件</dt>
        <dd>
          {summary.counts.attachments} / {summary.counts.files}
        </dd>
      </div>
    </dl>
  );
}

function DataManagementPage({ changed }: { changed: () => void }) {
  const [busy, setBusy] = useState<"export" | "inspect" | "restore">();
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");
  const [pending, setPending] = useState<{
    path: string;
    summary: WorkspaceBackupSummary;
  }>();

  const runExport = async () => {
    setBusy("export");
    setError("");
    setMessage("");
    try {
      const result = await exportWorkspaceBackup();
      if (result) setMessage(`备份已导出：${result.path}`);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setBusy(undefined);
    }
  };
  const chooseImport = async () => {
    setBusy("inspect");
    setError("");
    setMessage("");
    try {
      setPending(await chooseWorkspaceBackup());
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setBusy(undefined);
    }
  };
  const confirmRestore = async () => {
    if (!pending) return;
    setBusy("restore");
    setError("");
    try {
      const restored = await restoreWorkspaceBackup(pending.path);
      setPending(undefined);
      setMessage(
        `工作区已恢复。导入前的自动备份：${restored.recoveryBackupPath}`,
      );
      changed();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setBusy(undefined);
    }
  };

  return (
    <section className="page data-management-page">
      <header>
        <div>
          <p className="eyebrow">LOCAL WORKSPACE</p>
          <h1>数据管理</h1>
          <p className="muted">
            导出或恢复完整的 SQLite、附件、Sample lineage 与用户 Protocol。
          </p>
        </div>
      </header>
      <div className="data-management-grid">
        <article>
          <span className="data-management-icon">↑</span>
          <h2>导出工作区备份</h2>
          <p>
            生成一个可迁移的 <code>.labflow-backup</code>
            文件。导出使用 SQLite 一致性快照，不停止当前工作区。
          </p>
          <button
            className="primary"
            disabled={busy !== undefined}
            onClick={() => void runExport()}
          >
            {busy === "export" ? "正在导出…" : "一键导出"}
          </button>
        </article>
        <article>
          <span className="data-management-icon">↓</span>
          <h2>从备份恢复</h2>
          <p>
            导入前会校验数据库、外键、相对路径和每个文件的
            SHA-256，不会合并两个工作区。
          </p>
          <button
            className="secondary"
            disabled={busy !== undefined}
            onClick={() => void chooseImport()}
          >
            {busy === "inspect" ? "正在校验…" : "选择备份文件"}
          </button>
        </article>
      </div>
      {message && <p className="backup-success">{message}</p>}
      {error && <p className="form-error backup-error">{error}</p>}
      {pending && (
        <div className="overlay centered backup-confirm-overlay">
          <section
            className="modal backup-confirm"
            role="dialog"
            aria-modal="true"
            aria-labelledby="backup-confirm-title"
          >
            <h2 id="backup-confirm-title">恢复这个工作区？</h2>
            <p>
              恢复会完整替换当前工作区，不会合并数据。系统会先自动导出当前工作区作为恢复点。
            </p>
            <BackupSummaryView summary={pending.summary} />
            <div className="backup-confirm-actions">
              <button
                className="secondary"
                disabled={busy === "restore"}
                onClick={() => setPending(undefined)}
              >
                取消
              </button>
              <button
                className="danger"
                disabled={busy === "restore"}
                onClick={() => void confirmRestore()}
              >
                {busy === "restore" ? "正在恢复…" : "确认替换当前工作区"}
              </button>
            </div>
          </section>
        </div>
      )}
    </section>
  );
}

function Calendar({
  store,
  week,
  setWeek,
  openExisting,
  create,
}: {
  store: Store;
  week: Date;
  setWeek: (d: Date) => void;
  openExisting: (task: Task) => void;
  create: () => void;
}) {
  const days = useMemo(
    () => Array.from({ length: 7 }, (_, i) => addDays(week, i)),
    [week],
  );
  const end = addDays(week, 6);
  const format = new Intl.DateTimeFormat("zh-CN", {
    month: "long",
    day: "numeric",
  });
  return (
    <section className="page">
      <header>
        <div>
          <p className="eyebrow">CALENDAR</p>
          <h1>实验日历</h1>
          <p className="muted">安排实验任务，关联实验与实验记录。</p>
        </div>
        <button className="primary" onClick={create}>
          ＋ 新建任务
        </button>
      </header>
      <div className="calendar-toolbar">
        <div className="seg">
          <button onClick={() => setWeek(addDays(week, -7))}>‹</button>
          <button onClick={() => setWeek(startOfWeek(new Date()))}>今天</button>
          <button onClick={() => setWeek(addDays(week, 7))}>›</button>
        </div>
        <h2>
          {format.format(week)} – {format.format(end)}
        </h2>
      </div>
      <div className="calendar">
        <div className="time-col">
          <span>时间</span>
          {HOURS.map((hour) => (
            <label key={hour}>{String(hour).padStart(2, "0")}:00</label>
          ))}
        </div>
        {days.map((day) => (
          <Day
            key={day.toISOString()}
            day={day}
            tasks={store.tasks.filter((t) => sameDate(t.start, day))}
            experiments={store.experiments}
            open={openExisting}
          />
        ))}
      </div>
    </section>
  );
}

function Day({
  day,
  tasks,
  experiments,
  open,
}: {
  day: Date;
  tasks: Task[];
  experiments: Experiment[];
  open: (task: Task) => void;
}) {
  const today = sameDate(localDateTime(new Date()), day);
  return (
    <div className="day-col">
      <div className="day-head">
        {dayLabel(day.toISOString())}
        {today && <b>{day.getDate()}</b>}
      </div>
      {tasks.map((task) => {
        const start = new Date(task.start),
          end = new Date(task.end);
        const top =
          51 + (start.getHours() + start.getMinutes() / 60) * HOUR_HEIGHT;
        const height = Math.max(
          30,
          ((end.getTime() - start.getTime()) / 3600000) * HOUR_HEIGHT - 4,
        );
        const experiment = experiments.find((x) => x.id === task.experimentId);
        return (
          <button
            key={task.id}
            className="task-card"
            style={
              {
                top,
                height,
                "--task-color": experiment?.color || "#6957e8",
              } as React.CSSProperties
            }
            onClick={() => open(task)}
          >
            <i className={`dot ${task.status}`} />
            <b>{task.title}</b>
            <span>
              {experiment?.title || "未归属实验"} · {formatTime(task.start)}
            </span>
          </button>
        );
      })}
    </div>
  );
}

function TaskDrawer({
  task,
  experiment,
  samples,
  sampleTypes,
  protocols,
  tasks,
  records,
  close,
  edit,
  openRecord,
  changed,
  protocolsChanged,
}: {
  task: Task;
  experiment?: Experiment;
  samples: Store["samples"];
  sampleTypes: Store["sampleTypes"];
  protocols: Store["protocols"];
  tasks: Store["tasks"];
  records: Store["records"];
  close: () => void;
  edit: () => void;
  openRecord: () => void;
  changed: () => void;
  protocolsChanged: () => void;
}) {
  const [error, setError] = useState("");
  const [choosingProtocol, setChoosingProtocol] = useState(false);
  const [protocolQuery, setProtocolQuery] = useState("");
  const [creatingProtocol, setCreatingProtocol] = useState(false);
  const [protocol, setProtocol] = useState<Protocol>();
  const [values, setValues] = useState<Record<string, string>>({});
  const [inputSampleIds, setInputSampleIds] = useState<string[]>([]);
  const [inputMode, setInputMode] = useState<"existing" | "external">(
    "existing",
  );
  const [activeSampleGroup, setActiveSampleGroup] =
    useState<SampleSourceKind>("direct_parent");
  const [externalSampleType, setExternalSampleType] = useState("");
  const [externalSampleCount, setExternalSampleCount] = useState(1);
  const [externalSamples, setExternalSamples] = useState<
    { displayName: string; conditions: string }[]
  >([{ displayName: "", conditions: "" }]);
  const [plateGroups, setPlateGroups] = useState<PlateTreatmentGroup[]>([
    { factor: "", duration: "", wellCount: 1 },
  ]);
  const protocolResults = searchProtocols(protocols, protocolQuery);
  const closeProtocolPicker = () => {
    setChoosingProtocol(false);
    setCreatingProtocol(false);
    setProtocol(undefined);
    setProtocolQuery("");
  };
  const selectedInput = samples.find(
    (sample) => sample.id === values.input_sample,
  );
  const selectedInputType =
    (selectedInput && normalizeSampleType(selectedInput.type)) ||
    ({ 孔板: "PLATE", 培养皿: "DISH", 孔: "WELL" }[values.new_object_type] as
      string | undefined);
  const selectedPlateCapacity =
    plateCapacity(selectedInput?.metadata?.plate_capacity) ||
    plateCapacity(selectedInput?.metadata?.plate_format) ||
    plateCapacity(selectedInput?.metadata?.container_name) ||
    plateCapacity(values.new_plate_format);
  const usesExperimentSampleInput = [
    "parent_task_outputs",
    "experiment_samples",
  ].includes(protocol?.execution?.inputSource || "");
  const eligibleInputSamples = samples.filter((sample) => {
    if (!usesExperimentSampleInput) return false;
    if (sample.consumed) return false;
    if (sample.experimentId !== task.experimentId) return false;
    if (
      !(protocol?.execution?.inputTypes ?? [])
        .map(normalizeSampleType)
        .includes(normalizeSampleType(sample.type))
    )
      return false;
    return true;
  });
  const sampleGroups = groupSamplesBySource(
    eligibleInputSamples,
    task,
    tasks,
    records,
  );
  const sampleGroupLabels: Record<SampleSourceKind, string> = {
    direct_parent: "直接上级 Task 输出",
    other_task: "其他 Task 输出",
    external: "外部登记 Sample",
  };
  const selectedInGroup = (kind: SampleSourceKind) =>
    sampleGroups[kind].filter((sample) => inputSampleIds.includes(sample.id))
      .length;
  const toggleInputSample = (sampleId: string, selected: boolean) => {
    setInputMode("existing");
    setInputSampleIds((current) =>
      selected
        ? current.includes(sampleId)
          ? current
          : [...current, sampleId]
        : current.filter((id) => id !== sampleId),
    );
    setError("");
  };
  const renderSampleOption = (sample: Store["samples"][number]) => {
    const source = sampleSourceInfo(sample, task, tasks, records);
    const sourceText = source.sourceTask
      ? `来源：${source.sourceTask.title} · ${dayLabel(source.sourceTask.start)} ${formatTime(source.sourceTask.start)}`
      : source.kind === "external"
        ? "外部登记 · 无来源 Task"
        : "来源 Task 不可用";
    return (
      <label className="sample-source-option" key={sample.id}>
        <input
          type="checkbox"
          checked={inputSampleIds.includes(sample.id)}
          onChange={(event) =>
            toggleInputSample(sample.id, event.target.checked)
          }
        />
        <span>
          <span className={`sample-source-badge ${source.kind}`}>
            {source.kind === "direct_parent"
              ? "直接上级"
              : source.kind === "other_task"
                ? "其他 Task"
                : "外部登记"}
          </span>
          <b>{sample.code}</b>
          <small>
            {sample.displayName || sampleTypeLabel(sample.type)} · {sourceText}
            {sample.metadata?.treatment_factor
              ? ` · ${String(sample.metadata.treatment_factor)}`
              : ""}
            {sample.metadata?.treatment_duration
              ? ` · ${String(sample.metadata.treatment_duration)}`
              : ""}
          </small>
        </span>
      </label>
    );
  };
  const selectProtocol = (item: Protocol) => {
    setProtocol(item);
    setError("");
    setInputSampleIds([]);
    setInputMode("existing");
    setExternalSampleType(item.execution?.inputTypes?.[0] || "");
    setExternalSampleCount(1);
    setExternalSamples([{ displayName: "", conditions: "" }]);
    const inputTypes = (item.execution?.inputTypes || []).map(
      normalizeSampleType,
    );
    const candidates = samples.filter(
      (sample) =>
        !sample.consumed &&
        sample.experimentId === task.experimentId &&
        inputTypes.includes(normalizeSampleType(sample.type)),
    );
    const groups = groupSamplesBySource(candidates, task, tasks, records);
    setActiveSampleGroup(
      groups.direct_parent.length
        ? "direct_parent"
        : groups.other_task.length
          ? "other_task"
          : "external",
    );
    setValues(
      Object.fromEntries(
        (item.fields || [])
          .filter((field) => field.defaultValue !== undefined)
          .map((field) => [field.key, field.defaultValue || ""]),
      ),
    );
  };
  const complete = async () => {
    try {
      await updateTaskStatus(task.id, "completed");
      changed();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  };
  const start = async () => {
    if (!protocol) return;
    if (
      usesExperimentSampleInput &&
      inputMode === "existing" &&
      inputSampleIds.length === 0
    )
      return setError("请从当前 Experiment 中选择至少一个 Sample。");
    if (
      usesExperimentSampleInput &&
      inputMode === "external" &&
      (!externalSampleType ||
        externalSamples.length === 0 ||
        externalSamples.some((sample) => !sample.displayName.trim()))
    )
      return setError("请填写所有迁入 Sample 的类型和 Label。");
    if (
      protocol.fields?.some(
        (field) => field.required && !values[field.key]?.trim(),
      )
    )
      return setError("请填写所有必填字段。");
    if (protocol.execution?.eventType === "plating") {
      if (
        values.container_type === "孔板" &&
        !plateCapacity(values.plate_format)
      )
        return setError("请选择孔板规格。");
    }
    if (
      protocol.execution?.eventType === "treatment" &&
      selectedInputType === "PLATE"
    ) {
      const used = plateGroups.reduce(
        (total, group) => total + group.wellCount,
        0,
      );
      if (!selectedPlateCapacity) return setError("所选孔板缺少孔板规格。");
      if (plateGroups.length === 0) return setError("请增加至少一个刺激分组。");
      const invalidGroup = plateGroups.findIndex(
        (group) =>
          !group.factor.trim() ||
          !group.duration.trim() ||
          !Number.isInteger(group.wellCount) ||
          group.wellCount < 1,
      );
      if (invalidGroup >= 0) {
        const group = plateGroups[invalidGroup];
        const missing = [
          !group.factor.trim() && "刺激因素",
          !group.duration.trim() && "刺激时间",
          (!Number.isInteger(group.wellCount) || group.wellCount < 1) && "孔数",
        ].filter(Boolean);
        return setError(
          `第 ${invalidGroup + 1} 组缺少或未正确填写：${missing.join("、")}。`,
        );
      }
      if (used > selectedPlateCapacity)
        return setError(
          `已分配 ${used} 孔，超过 ${selectedPlateCapacity} 孔板容量。`,
        );
    }
    try {
      const submittedValues =
        protocol.execution?.eventType === "treatment" &&
        selectedInputType === "PLATE"
          ? { ...values, treatment_groups: JSON.stringify(plateGroups) }
          : values;
      await startTaskRecord(
        task.id,
        protocol.id,
        uid("rec"),
        submittedValues,
        usesExperimentSampleInput && inputMode === "existing"
          ? inputSampleIds
          : values.input_sample
            ? [values.input_sample]
            : [],
        usesExperimentSampleInput && inputMode === "external"
          ? externalSamples.map((sample): ExternalSampleDraft => ({
              sampleType: externalSampleType,
              displayName: sample.displayName.trim(),
              metadata: sample.conditions.trim()
                ? { existing_conditions: sample.conditions.trim() }
                : {},
            }))
          : [],
      );
      changed();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  };
  return (
    <div className="overlay">
      <aside className="drawer">
        <button className="close" onClick={close} aria-label="关闭">
          ×
        </button>
        <span className={`status ${task.status}`}>
          {statusLabel[task.status]}
        </span>
        <h2>{task.title}</h2>
        <p className="drawer-exp">
          <i style={{ background: experiment?.color || "#6957e8" }} />
          {experiment?.title || "未归属实验"}
        </p>
        <dl>
          <div>
            <dt>时间</dt>
            <dd>
              {dayLabel(task.start)} · {formatTime(task.start)}–
              {formatTime(task.end)}
            </dd>
          </div>
        </dl>
        <button
          className="primary wide"
          onClick={
            task.recordId
              ? openRecord
              : () => {
                  setProtocolQuery("");
                  setChoosingProtocol(true);
                }
          }
        >
          打开记录 →
        </button>
        <button className="secondary wide" onClick={edit}>
          修改任务
        </button>
        {task.status !== "completed" && (
          <button className="secondary wide" onClick={() => void complete()}>
            ✓ 标记为完成
          </button>
        )}
        {error && <p className="form-error">{error}</p>}
        {choosingProtocol && (
          <div className="overlay centered">
            <div className={`modal ${protocol ? "" : "protocol-search-modal"}`}>
              <button
                className="close"
                onClick={closeProtocolPicker}
                aria-label="关闭 Protocol 选择"
              >
                ×
              </button>
              {!protocol ? (
                <>
                  <p className="eyebrow">OPEN RECORD</p>
                  <h2>选择 Protocol</h2>
                  <label className="protocol-search-field">
                    <span>搜索 Protocol</span>
                    <input
                      autoFocus
                      type="search"
                      value={protocolQuery}
                      onChange={(event) => setProtocolQuery(event.target.value)}
                      placeholder="输入名称、描述或分类"
                    />
                  </label>
                  {!protocolQuery.trim() ? (
                    <div className="protocol-search-prompt">
                      输入关键词后显示匹配的 Protocol。
                    </div>
                  ) : protocolResults.length ? (
                    <div className="protocol-search-results">
                      <small>找到 {protocolResults.length} 个 Protocol</small>
                      {protocolResults.map((item) => (
                        <button
                          className="picker"
                          onClick={() => selectProtocol(item)}
                          key={item.id}
                        >
                          <div>
                            <b>{item.name}</b>
                            <small>
                              {item.category}
                              {item.description ? ` · ${item.description}` : ""}
                            </small>
                          </div>
                          →
                        </button>
                      ))}
                    </div>
                  ) : (
                    <div className="protocol-search-empty">
                      <b>没有找到“{protocolQuery.trim()}”</b>
                      <p>可以创建新的 Protocol，并继续用于当前 Record。</p>
                      <button
                        className="primary"
                        onClick={() => setCreatingProtocol(true)}
                      >
                        ＋ 新增 Protocol
                      </button>
                    </div>
                  )}
                </>
              ) : (
                <>
                  <h2>{protocol.name}</h2>
                  {usesExperimentSampleInput && (
                    <fieldset className="protocol-inputs">
                      <legend>1. 本次实验使用什么 Sample？</legend>
                      {inputMode === "existing" &&
                        inputSampleIds.length > 0 && (
                          <div className="selected-sample-summary">
                            <b>已选择 {inputSampleIds.length} 个 Sample</b>
                            <span>
                              {inputSampleIds
                                .map(
                                  (id) =>
                                    samples.find((sample) => sample.id === id)
                                      ?.code,
                                )
                                .filter(Boolean)
                                .join("、")}
                            </span>
                          </div>
                        )}
                      <div className="sample-source-groups">
                        {(
                          [
                            "direct_parent",
                            "other_task",
                            "external",
                          ] as SampleSourceKind[]
                        ).map((kind, index) => (
                          <button
                            type="button"
                            className={
                              activeSampleGroup === kind ? "active" : ""
                            }
                            key={kind}
                            onClick={() => setActiveSampleGroup(kind)}
                          >
                            <span>
                              <i>{index + 1}</i>
                              <b>{sampleGroupLabels[kind]}</b>
                            </span>
                            <small>
                              {sampleGroups[kind].length} 个可用
                              {selectedInGroup(kind) > 0
                                ? ` · 已选 ${selectedInGroup(kind)}`
                                : ""}
                            </small>
                            <em>{activeSampleGroup === kind ? "▼" : "▶"}</em>
                          </button>
                        ))}
                      </div>
                      <div className="sample-source-panel">
                        {activeSampleGroup !== "external" && (
                          <>
                            {sampleGroups[activeSampleGroup].map(
                              renderSampleOption,
                            )}
                            {sampleGroups[activeSampleGroup].length === 0 && (
                              <p className="form-hint">
                                该来源中没有符合此 Protocol 的可用 Sample。
                              </p>
                            )}
                          </>
                        )}
                        {activeSampleGroup === "external" && (
                          <>
                            {inputMode === "existing" &&
                              sampleGroups.external.map(renderSampleOption)}
                            {inputMode === "existing" &&
                              sampleGroups.external.length === 0 && (
                                <p className="form-hint">
                                  当前 Experiment 没有已登记的外部 Sample。
                                </p>
                              )}
                            {inputMode === "existing" ? (
                              <button
                                className="link-button register-external-button"
                                type="button"
                                onClick={() => {
                                  setInputMode("external");
                                  setInputSampleIds([]);
                                  setError("");
                                }}
                              >
                                ＋ 登记新的当前已有 Sample
                              </button>
                            ) : (
                              <div className="external-sample-form">
                                <button
                                  className="link-button"
                                  type="button"
                                  onClick={() => {
                                    setInputMode("existing");
                                    setError("");
                                  }}
                                >
                                  ← 选择已登记的外部 Sample
                                </button>
                                <label className="task-form">
                                  Sample type
                                  <select
                                    value={externalSampleType}
                                    onChange={(event) =>
                                      setExternalSampleType(event.target.value)
                                    }
                                  >
                                    {(protocol.execution?.inputTypes || []).map(
                                      (sampleType) => (
                                        <option
                                          key={sampleType}
                                          value={sampleType}
                                        >
                                          {sampleTypeLabel(sampleType)}
                                        </option>
                                      ),
                                    )}
                                  </select>
                                </label>
                                <label className="task-form">
                                  数量
                                  <input
                                    type="number"
                                    min="1"
                                    max="96"
                                    value={externalSampleCount}
                                    onChange={(event) => {
                                      const count = Math.max(
                                        1,
                                        Math.min(
                                          96,
                                          Number(event.target.value) || 1,
                                        ),
                                      );
                                      setExternalSampleCount(count);
                                      setExternalSamples((current) =>
                                        Array.from(
                                          { length: count },
                                          (_, index) =>
                                            current[index] || {
                                              displayName: "",
                                              conditions: "",
                                            },
                                        ),
                                      );
                                    }}
                                  />
                                </label>
                                {externalSamples.map((sample, index) => (
                                  <div
                                    className="external-sample-row"
                                    key={index}
                                  >
                                    <b>Sample {index + 1}</b>
                                    <label>
                                      Label
                                      <input
                                        value={sample.displayName}
                                        onChange={(event) =>
                                          setExternalSamples((current) =>
                                            current.map((item, itemIndex) =>
                                              itemIndex === index
                                                ? {
                                                    ...item,
                                                    displayName:
                                                      event.target.value,
                                                  }
                                                : item,
                                            ),
                                          )
                                        }
                                      />
                                    </label>
                                    <label>
                                      已有实验条件（可选）
                                      <input
                                        placeholder="例如 siNC，24 h"
                                        value={sample.conditions}
                                        onChange={(event) =>
                                          setExternalSamples((current) =>
                                            current.map((item, itemIndex) =>
                                              itemIndex === index
                                                ? {
                                                    ...item,
                                                    conditions:
                                                      event.target.value,
                                                  }
                                                : item,
                                            ),
                                          )
                                        }
                                      />
                                    </label>
                                  </div>
                                ))}
                              </div>
                            )}
                          </>
                        )}
                      </div>
                    </fieldset>
                  )}
                  {protocol.fields?.map((field) => {
                    const visible =
                      (!field.visibleWhen ||
                        values[field.visibleWhen.key] ===
                          field.visibleWhen.value) &&
                      (!field.visibleForInputTypes ||
                        (!!selectedInputType &&
                          field.visibleForInputTypes.includes(
                            selectedInputType,
                          )));
                    if (!visible) return null;
                    if (field.kind === "plate_layout")
                      return (
                        <div className="task-form" key={field.key}>
                          <span>{field.label}</span>
                          <PlateLayoutEditor
                            capacity={selectedPlateCapacity}
                            groups={plateGroups}
                            onChange={(groups) => {
                              setPlateGroups(groups);
                              setError("");
                            }}
                          />
                        </div>
                      );
                    return (
                      <label className="task-form" key={field.key}>
                        {field.label}
                        {field.kind === "samples" ? (
                          <select
                            value={values[field.key] || ""}
                            onChange={(e) =>
                              setValues({
                                ...values,
                                [field.key]: e.target.value,
                              })
                            }
                          >
                            <option value="">新建对象（不选择现有样本）</option>
                            {samples
                              .filter(
                                (sample) =>
                                  !sample.consumed &&
                                  sample.experimentId === task.experimentId &&
                                  (protocol.execution?.inputTypes ?? [])
                                    .map(normalizeSampleType)
                                    .includes(normalizeSampleType(sample.type)),
                              )
                              .map((sample) => (
                                <option value={sample.id} key={sample.id}>
                                  {sample.code}
                                </option>
                              ))}
                          </select>
                        ) : field.kind === "select" ? (
                          <select
                            value={values[field.key] || ""}
                            onChange={(e) =>
                              setValues({
                                ...values,
                                [field.key]: e.target.value,
                              })
                            }
                          >
                            <option value="">请选择</option>
                            {field.options?.map((option) => (
                              <option key={option}>{option}</option>
                            ))}
                          </select>
                        ) : (
                          <input
                            value={values[field.key] || ""}
                            onChange={(e) =>
                              setValues({
                                ...values,
                                [field.key]: e.target.value,
                              })
                            }
                          />
                        )}
                      </label>
                    );
                  })}
                  {error && (
                    <p className="form-error protocol-error">{error}</p>
                  )}
                  <button className="primary wide" onClick={() => void start()}>
                    创建实验记录
                  </button>
                </>
              )}
            </div>
          </div>
        )}
        {creatingProtocol && (
          <ProtocolCreationWizard
            sampleTypes={sampleTypes}
            initialName={protocolQuery.trim()}
            close={() => setCreatingProtocol(false)}
            saved={protocolsChanged}
          />
        )}
      </aside>
    </div>
  );
}

function PlateLayoutEditor({
  capacity,
  groups,
  onChange,
}: {
  capacity: number;
  groups: PlateTreatmentGroup[];
  onChange: (groups: PlateTreatmentGroup[]) => void;
}) {
  const used = groups.reduce((total, group) => total + group.wellCount, 0);
  const update = (index: number, patch: Partial<PlateTreatmentGroup>) =>
    onChange(
      groups.map((group, groupIndex) =>
        groupIndex === index ? { ...group, ...patch } : group,
      ),
    );
  return (
    <div className="plate-layout">
      <div className={`plate-capacity ${used > capacity ? "over" : ""}`}>
        <b>{capacity ? `${capacity} 孔板` : "孔板规格缺失"}</b>
        <span>
          已分配 {used} / {capacity || "?"} 孔
        </span>
      </div>
      <div className="plate-group plate-group-head" aria-hidden="true">
        <span />
        <b>刺激因素（必填）</b>
        <b>刺激时间（必填）</b>
        <b>孔数（必填）</b>
        <span />
      </div>
      {groups.map((group, index) => (
        <div className="plate-group" key={index}>
          <span>{index + 1}</span>
          <input
            aria-label={`第 ${index + 1} 组刺激因素`}
            placeholder="刺激因素，如 si NC"
            value={group.factor}
            onChange={(event) => update(index, { factor: event.target.value })}
          />
          <input
            aria-label={`第 ${index + 1} 组刺激时间`}
            placeholder="刺激时间，如 24h"
            value={group.duration}
            onChange={(event) =>
              update(index, { duration: event.target.value })
            }
          />
          <input
            aria-label={`第 ${index + 1} 组孔数`}
            type="number"
            min="1"
            max={capacity || 384}
            value={group.wellCount}
            onChange={(event) =>
              update(index, { wellCount: Number(event.target.value) })
            }
          />
          <button
            type="button"
            aria-label={`删除第 ${index + 1} 组`}
            disabled={groups.length === 1}
            onClick={() => onChange(groups.filter((_, item) => item !== index))}
          >
            ×
          </button>
        </div>
      ))}
      <button
        type="button"
        className="add-plate-group"
        onClick={() =>
          onChange([...groups, { factor: "", duration: "", wellCount: 1 }])
        }
      >
        ＋ 增加刺激分组
      </button>
      {capacity > 0 && used > capacity && (
        <p className="form-error">分组孔数不能超过孔板容量。</p>
      )}
    </div>
  );
}

function ExperimentsPage({
  store,
  openTask,
}: {
  store: Store;
  openTask: (task: Task) => void;
}) {
  const [selectedExperimentId, setSelectedExperimentId] = useState<string>();
  const selectedExperiment = store.experiments.find(
    (experiment) => experiment.id === selectedExperimentId,
  );
  const experimentTasks = useMemo(
    () =>
      store.tasks.filter((task) => task.experimentId === selectedExperimentId),
    [selectedExperimentId, store.tasks],
  );
  const graph = useMemo(
    () => buildTaskGraph(experimentTasks),
    [experimentTasks],
  );

  if (!selectedExperiment) {
    return (
      <section className="page">
        <header>
          <div>
            <p className="eyebrow">EXPERIMENTS</p>
            <h1>实验</h1>
            <p className="muted">
              查看每个 Experiment 内 Task 的只读网状关系。
            </p>
          </div>
        </header>
        <div className="experiment-grid">
          {store.experiments.map((experiment) => {
            const tasks = store.tasks
              .filter((task) => task.experimentId === experiment.id)
              .sort((left, right) => left.start.localeCompare(right.start));
            const taskIds = new Set(tasks.map((task) => task.id));
            const relationCount = tasks.reduce(
              (count, task) =>
                count +
                (task.parentTaskIds || []).filter((id) => taskIds.has(id))
                  .length,
              0,
            );
            const completed = tasks.filter(
              (task) => task.status === "completed",
            ).length;
            const completion = tasks.length
              ? Math.round((completed / tasks.length) * 100)
              : 0;
            return (
              <button
                className="experiment-card experiment-open"
                key={experiment.id}
                onClick={() => setSelectedExperimentId(experiment.id)}
              >
                <div className="card-top">
                  <i style={{ background: experiment.color }}>◈</i>
                  <span>{experiment.code}</span>
                  <b>查看网络 →</b>
                </div>
                <h2>{experiment.title}</h2>
                <p>{experiment.description || "暂无实验描述。"}</p>
                <div className="progress">
                  <span>
                    {tasks.length} Tasks · {relationCount} 条关系 · {completed}{" "}
                    已完成
                  </span>
                  <div>
                    <i
                      style={{
                        width: `${completion}%`,
                        background: experiment.color,
                      }}
                    />
                  </div>
                </div>
              </button>
            );
          })}
          {store.experiments.length === 0 && (
            <div className="empty">暂无 Experiment。</div>
          )}
        </div>
      </section>
    );
  }

  const orderedTasks = [...experimentTasks].sort((left, right) =>
    left.start.localeCompare(right.start),
  );
  const firstDate = orderedTasks[0]?.start.slice(0, 10);
  const lastDate = orderedTasks.at(-1)?.start.slice(0, 10);
  return (
    <section className="page experiment-detail">
      <header>
        <div className="experiment-detail-title">
          <button
            className="back"
            onClick={() => setSelectedExperimentId(undefined)}
            aria-label="返回 Experiment 列表"
          >
            ←
          </button>
          <div>
            <p className="eyebrow">{selectedExperiment.code}</p>
            <h1>{selectedExperiment.title}</h1>
            <p className="muted">
              {experimentTasks.length} Tasks
              {firstDate &&
                ` · ${firstDate}${lastDate !== firstDate ? ` — ${lastDate}` : ""}`}
            </p>
          </div>
        </div>
        <span className="readonly-badge">只读 Task 网络</span>
      </header>
      <div className="task-graph-legend" aria-label="Task 状态图例">
        <span>
          <i className="planned" />
          计划中
        </span>
        <span>
          <i className="in_progress" />
          进行中
        </span>
        <span>
          <i className="completed" />
          已完成
        </span>
        <span>
          <b>→</b>依赖关系
        </span>
      </div>
      {(graph.hasCycle || graph.invalidRelationCount > 0) && (
        <p className="graph-warning" role="status">
          检测到异常 Task 关系：
          {graph.hasCycle && "存在循环依赖；已使用安全降级布局。"}
          {graph.invalidRelationCount > 0 &&
            `已忽略 ${graph.invalidRelationCount} 条无效关系。`}
        </p>
      )}
      {experimentTasks.length > 0 ? (
        <div className="task-graph-frame">
          <div
            className="task-graph-canvas"
            style={{ width: graph.width, height: graph.height }}
          >
            <svg
              aria-hidden="true"
              width={graph.width}
              height={graph.height}
              viewBox={`0 0 ${graph.width} ${graph.height}`}
            >
              <defs>
                <marker
                  id="task-graph-arrow"
                  markerWidth="8"
                  markerHeight="8"
                  refX="7"
                  refY="4"
                  orient="auto"
                >
                  <path d="M 0 0 L 8 4 L 0 8 z" />
                </marker>
              </defs>
              {graph.edges.map((edge) => (
                <path
                  className="task-graph-edge"
                  d={edge.path}
                  key={edge.id}
                  markerEnd="url(#task-graph-arrow)"
                />
              ))}
            </svg>
            {graph.nodes.map((node) => {
              const record =
                store.records.find(
                  (candidate) => candidate.id === node.task.recordId,
                ) ||
                store.records.find(
                  (candidate) => candidate.taskId === node.task.id,
                );
              const protocol = store.protocols.find(
                (candidate) => candidate.id === record?.protocolId,
              );
              return (
                <button
                  className={`task-graph-node ${node.task.status} ${node.connected ? "connected" : "isolated"}`}
                  style={
                    {
                      left: node.x,
                      top: node.y,
                      width: TASK_GRAPH_NODE_WIDTH,
                      height: TASK_GRAPH_NODE_HEIGHT,
                      "--experiment-color": selectedExperiment.color,
                    } as React.CSSProperties
                  }
                  key={node.task.id}
                  onClick={() => openTask(node.task)}
                  aria-label={`打开 Task：${node.task.title}`}
                >
                  <span className="task-graph-node-top">
                    <i />
                    {dayLabel(node.task.start)} · {formatTime(node.task.start)}
                  </span>
                  <b>{node.task.title}</b>
                  <small>
                    {record
                      ? record.protocolName || protocol?.name || "已有 Record"
                      : "尚无 Record"}
                  </small>
                  {!node.connected && <em>未关联</em>}
                </button>
              );
            })}
          </div>
        </div>
      ) : (
        <div className="empty experiment-graph-empty">
          当前 Experiment 暂无 Task。
        </div>
      )}
      <p className="graph-help">
        连线来自已保存的 Task 上级关系；点击节点可打开现有 Task
        详情。本图不会修改任何关系。
      </p>
    </section>
  );
}

function ProtocolsPage({
  protocols,
  sampleTypes,
  records,
  changed,
}: {
  protocols: Store["protocols"];
  sampleTypes: Store["sampleTypes"];
  records: Store["records"];
  changed: () => void;
}) {
  const [creating, setCreating] = useState(false);
  const [editing, setEditing] = useState<Protocol>();
  const [deletingProtocol, setDeletingProtocol] = useState<Protocol>();
  const [deleteError, setDeleteError] = useState("");
  const [deleting, setDeleting] = useState(false);
  const referencedRecordCount = deletingProtocol
    ? records.filter((record) => record.protocolId === deletingProtocol.id)
        .length
    : 0;
  const removeProtocol = async () => {
    if (!deletingProtocol) return;
    setDeleting(true);
    setDeleteError("");
    try {
      await deleteProtocol(deletingProtocol.id);
      setDeletingProtocol(undefined);
      changed();
    } catch (reason) {
      setDeleteError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setDeleting(false);
    }
  };
  return (
    <section className="page">
      <header>
        <div>
          <p className="eyebrow">PROTOCOL LIBRARY</p>
          <h1>实验 Protocol</h1>
          <p className="muted">结构化模板会在创建记录时保存版本快照。</p>
        </div>
        <button className="primary" onClick={() => setCreating(true)}>
          ＋ 新增 Protocol
        </button>
      </header>
      <div className="protocol-grid">
        {protocols.map((protocol) => (
          <article className="protocol-card" key={protocol.id}>
            <i
              style={{
                background: `${protocol.accent}18`,
                color: protocol.accent,
              }}
            >
              ⌁
            </i>
            <p>{protocol.category}</p>
            <h2>{protocol.name}</h2>
            {protocol.description && <p>{protocol.description}</p>}
            <span>当前版本 v{protocol.version}</span>
            <div>
              {protocol.blocks.map((block) => (
                <em key={block}>{block}</em>
              ))}
            </div>
            <footer>
              <div className="protocol-card-actions">
                <button onClick={() => setEditing(protocol)}>
                  编辑 Record 正文
                </button>
                <button>查看版本 →</button>
                {protocol.origin === "user" && (
                  <button
                    className="danger"
                    onClick={() => {
                      setDeleteError("");
                      setDeletingProtocol(protocol);
                    }}
                  >
                    删除
                  </button>
                )}
              </div>
            </footer>
          </article>
        ))}
      </div>
      {creating && (
        <ProtocolCreationWizard
          sampleTypes={sampleTypes}
          close={() => setCreating(false)}
          saved={changed}
        />
      )}
      {editing && (
        <ProtocolTemplateEditor
          protocol={editing}
          close={() => setEditing(undefined)}
          saved={changed}
        />
      )}
      {deletingProtocol && (
        <div className="overlay centered protocol-delete-overlay">
          <section
            className="modal protocol-delete-confirm"
            role="dialog"
            aria-modal="true"
            aria-labelledby="protocol-delete-title"
          >
            <h2 id="protocol-delete-title">删除 Protocol？</h2>
            <p>
              确定删除“{deletingProtocol.name}”及其全部模板版本吗？
              {referencedRecordCount > 0
                ? `已有 ${referencedRecordCount} 条 Record 使用过它；这些 Record 将继续使用各自冻结的正文和 Protocol snapshot。`
                : "该 Protocol 尚未创建过 Record。"}
            </p>
            <p className="muted">已注册的 Sample Type 不会被删除。</p>
            {deleteError && <p className="form-error">{deleteError}</p>}
            <div className="protocol-delete-actions">
              <button
                className="secondary"
                disabled={deleting}
                onClick={() => {
                  setDeletingProtocol(undefined);
                  setDeleteError("");
                }}
              >
                取消
              </button>
              <button
                className="danger"
                disabled={deleting}
                onClick={() => void removeProtocol()}
              >
                {deleting ? "删除中…" : "确认删除"}
              </button>
            </div>
          </section>
        </div>
      )}
    </section>
  );
}

function RecordsPage({
  store,
  openedRecordId,
  closeRecord,
  openRecord,
  changed,
}: {
  store: Store;
  openedRecordId?: string;
  closeRecord: () => void;
  openRecord: (id: string) => void;
  changed: () => void;
}) {
  const record = store.records.find((item) => item.id === openedRecordId);
  const recordTerminalAssay = record?.protocolSnapshot?.schema?.terminalAssay;
  const taskForRecord = (recordId: string) => {
    const item = store.records.find((candidate) => candidate.id === recordId);
    return store.tasks.find((task) => task.id === item?.taskId);
  };
  const recordDate = (recordId: string) =>
    taskForRecord(recordId)?.start.slice(0, 10) || "";
  const sortedRecords = useMemo(
    () =>
      [...store.records].sort((left, right) => {
        const leftTask = store.tasks.find((task) => task.id === left.taskId);
        const rightTask = store.tasks.find((task) => task.id === right.taskId);
        return (
          (leftTask?.start || "").localeCompare(rightTask?.start || "") ||
          left.id.localeCompare(right.id)
        );
      }),
    [store.records, store.tasks],
  );
  const dates = [
    ...new Set(sortedRecords.map((item) => recordDate(item.id))),
  ].filter(Boolean);
  const [dateFrom, setDateFrom] = useState(dates[0] || "");
  const [dateTo, setDateTo] = useState(dates.at(-1) || "");
  const [selectedIds, setSelectedIds] = useState<Set<string>>(
    () => new Set(store.records.map((item) => item.id)),
  );
  const [exportPreview, setExportPreview] = useState<{
    records: typeof store.records;
    store: Store;
    manifest: Awaited<ReturnType<typeof createExportManifest>>;
  }>();
  const [exportError, setExportError] = useState("");
  const [pdfProgress, setPdfProgress] = useState("");
  const [pdfBusy, setPdfBusy] = useState(false);
  const [pdfFinishing, setPdfFinishing] = useState(false);
  const [printMode, setPrintMode] = useState(false);
  const pdfController = useRef<AbortController | undefined>(undefined);
  useEffect(() => () => pdfController.current?.abort(), []);
  const [deleteError, setDeleteError] = useState("");
  const [deleteConfirmOpen, setDeleteConfirmOpen] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [editingBody, setEditingBody] = useState(false);
  const [bodyDraft, setBodyDraft] = useState("");
  const [bodyError, setBodyError] = useState("");
  const [savingBody, setSavingBody] = useState(false);
  const [insertingImage, setInsertingImage] = useState(false);
  const bodyTextareaRef = useRef<HTMLTextAreaElement>(null);
  const closeRecordView = () => {
    setDeleteError("");
    setDeleteConfirmOpen(false);
    setDeleting(false);
    setEditingBody(false);
    setBodyDraft("");
    setBodyError("");
    setSavingBody(false);
    setInsertingImage(false);
    closeRecord();
  };
  const visibleRecords = sortedRecords.filter((item) => {
    const date = recordDate(item.id);
    return (!dateFrom || date >= dateFrom) && (!dateTo || date <= dateTo);
  });
  const groupedRecords = visibleRecords.reduce<
    { date: string; records: typeof store.records }[]
  >((groups, item) => {
    const date = recordDate(item.id);
    const last = groups.at(-1);
    if (last?.date === date) last.records.push(item);
    else groups.push({ date, records: [item] });
    return groups;
  }, []);
  const selectedRecords = visibleRecords.filter((item) =>
    selectedIds.has(item.id),
  );
  const toggleRecords = (ids: string[], selected: boolean) => {
    setSelectedIds((current) => {
      const next = new Set(current);
      ids.forEach((id) => (selected ? next.add(id) : next.delete(id)));
      return next;
    });
  };
  const previewExport = async () => {
    if (selectedRecords.length === 0) {
      setExportError("请至少选择一条实验记录。");
      return;
    }
    try {
      const manifest = await createExportManifest({
        id: uid("export"),
        dateFrom: recordDate(selectedRecords[0].id),
        dateTo: recordDate(selectedRecords.at(-1)!.id),
        recordIds: selectedRecords.map((item) => item.id),
        createdAt: new Date().toISOString(),
      });
      setExportPreview({ records: selectedRecords, store, manifest });
      setPdfProgress("");
      setExportError("");
    } catch (reason) {
      setExportError(reason instanceof Error ? reason.message : String(reason));
    }
  };
  const printExport = async () => {
    if (!exportPreview || printMode || pdfController.current) return;
    const imageCount = exportPreview.records.reduce(
      (count, item) =>
        count +
        parseRecordBody(item.renderedContent || item.notes || "").filter(
          (segment) => segment.type === "image",
        ).length,
      0,
    );
    if (imageCount > 8) {
      setExportError(
        "本次包含超过 8 张图片，请使用“低内存 PDF”，避免系统打印同时保留全部图片。",
      );
      return;
    }
    setPrintMode(true);
    setExportError("");
    try {
      // Let React mount the small, explicitly bounded print-only image set.
      await new Promise<void>((resolve) =>
        requestAnimationFrame(() => requestAnimationFrame(() => resolve())),
      );
      const images = Array.from(
        document.querySelectorAll<HTMLImageElement>(
          ".export-document img[data-record-image]",
        ),
      );
      if (images.length !== imageCount)
        throw new Error("正文引用的图片附件缺失");
      for (const image of images) await image.decode();
      await markExportPrintRequested(exportPreview.manifest.id);
      await Promise.resolve(window.print());
    } catch (reason) {
      setExportError(
        reason instanceof Error
          ? `导出前无法加载全部图片：${reason.message}`
          : String(reason),
      );
    } finally {
      setPrintMode(false);
    }
  };
  const lowMemoryExport = async () => {
    if (!exportPreview || pdfController.current || printMode) return;
    const controller = new AbortController();
    pdfController.current = controller;
    setPdfBusy(true);
    setExportError("");
    setPdfProgress("请选择保存位置…");
    let job: string | undefined;
    try {
      job = await beginRecordPdf();
      controller.signal.throwIfAborted();
      if (!job) {
        setPdfProgress("");
        return;
      }
      setPdfProgress("正在逐页生成 PDF…");
      const result = await renderRecordPdf(
        recordPdfBlocks(
          exportPreview.records,
          exportPreview.store,
          exportPreview.manifest,
        ),
        {
          signal: controller.signal,
          imageUrl: recordImagePreviewUrl,
          writePage: (jpeg, sequence) =>
            appendRecordPdfPage(job!, sequence, jpeg),
          progress: (pages, images) =>
            setPdfProgress(`已写入 ${pages} 页 · 已处理 ${images} 张图片`),
        },
      );
      controller.signal.throwIfAborted();
      setPdfFinishing(true);
      const path = await finishRecordPdf(job);
      job = undefined;
      setPdfProgress(
        `已保存 ${result.pages} 页、${result.images} 张图片：${path}`,
      );
      // Keep the existing manifest status vocabulary: this records a request,
      // not a new domain-level "PDF succeeded" state.
      try {
        await markExportPrintRequested(exportPreview.manifest.id);
      } catch (reason) {
        setExportError(`PDF 已保存，但导出审计状态更新失败：${String(reason)}`);
      }
    } catch (reason) {
      if (controller.signal.aborted)
        setPdfProgress("导出已取消，未完成文件已清理。");
      else
        setExportError(
          `PDF 导出失败：${reason instanceof Error ? reason.message : String(reason)}`,
        );
    } finally {
      if (job) {
        try {
          await cancelRecordPdf(job);
        } catch {
          setExportError(
            "PDF 临时文件清理失败，请重启 LabFlow 后检查保存目录中的 .labflow-*.pdf-part 文件。",
          );
        }
      }
      pdfController.current = undefined;
      setPdfBusy(false);
      setPdfFinishing(false);
    }
  };
  const removeRecord = async () => {
    if (!record) return;
    setDeleting(true);
    setDeleteError("");
    try {
      await deleteRecord(record.id);
      setDeleteConfirmOpen(false);
      closeRecordView();
      changed();
    } catch (reason) {
      setDeleteError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setDeleting(false);
    }
  };
  const beginBodyEdit = () => {
    if (!record) return;
    setBodyDraft(record.renderedContent || record.notes || "");
    setBodyError("");
    setEditingBody(true);
  };
  const saveBody = async () => {
    if (!record) return;
    if (!bodyDraft.trim()) {
      setBodyError("实验正文不能为空。");
      return;
    }
    setSavingBody(true);
    setBodyError("");
    try {
      await updateRecordBody(record.id, bodyDraft);
      setEditingBody(false);
      changed();
    } catch (reason) {
      setBodyError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setSavingBody(false);
    }
  };
  const addImageToBody = async () => {
    if (!record) return;
    try {
      const sourcePath = await chooseRecordImage();
      if (!sourcePath) return;
      const attachmentId = uid("attachment");
      const selection =
        bodyTextareaRef.current?.selectionStart ?? bodyDraft.length;
      const inserted = insertImageReference(
        bodyDraft,
        selection,
        attachmentId,
        imageCaptionFromPath(sourcePath),
      );
      setInsertingImage(true);
      setBodyError("");
      await insertRecordImage({
        id: attachmentId,
        recordId: record.id,
        sourcePath,
        renderedContent: inserted.content,
        changeId: uid("record-change"),
        createdAt: new Date().toISOString(),
      });
      setBodyDraft(inserted.content);
      changed();
      requestAnimationFrame(() => {
        bodyTextareaRef.current?.focus();
        bodyTextareaRef.current?.setSelectionRange(
          inserted.cursor,
          inserted.cursor,
        );
      });
    } catch (reason) {
      setBodyError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setInsertingImage(false);
    }
  };
  return (
    <section className="page">
      <header>
        <div>
          <p className="eyebrow">RECORDS</p>
          <h1>实验记录</h1>
          <p className="muted">
            按 Task 实验日期整理、选择并合并导出本地实验记录。
          </p>
        </div>
      </header>
      {store.records.length > 0 && (
        <div className="record-export-toolbar">
          <label>
            开始日期
            <input
              type="date"
              value={dateFrom}
              max={dateTo || undefined}
              onChange={(event) => setDateFrom(event.target.value)}
            />
          </label>
          <span>—</span>
          <label>
            结束日期
            <input
              type="date"
              value={dateTo}
              min={dateFrom || undefined}
              onChange={(event) => setDateTo(event.target.value)}
            />
          </label>
          <button
            className="secondary"
            onClick={() =>
              toggleRecords(
                visibleRecords.map((item) => item.id),
                true,
              )
            }
          >
            全选结果
          </button>
          <button
            className="secondary"
            onClick={() =>
              toggleRecords(
                visibleRecords.map((item) => item.id),
                false,
              )
            }
          >
            清空
          </button>
          <div>
            <b>{selectedRecords.length}</b> 条记录
          </div>
          <button className="primary" onClick={() => void previewExport()}>
            合并导出
          </button>
        </div>
      )}
      {exportError && <p className="form-error export-error">{exportError}</p>}
      <div className="record-list">
        {groupedRecords.map((group) => {
          const groupIds = group.records.map((item) => item.id);
          const allSelected = groupIds.every((id) => selectedIds.has(id));
          return (
            <section className="record-date-group" key={group.date}>
              <header>
                <label>
                  <input
                    type="checkbox"
                    checked={allSelected}
                    onChange={(event) =>
                      toggleRecords(groupIds, event.target.checked)
                    }
                  />
                  <b>{group.date}</b>
                </label>
                <span>{group.records.length} 条记录</span>
              </header>
              {group.records.map((item) => {
                const experiment = store.experiments.find(
                  (candidate) => candidate.id === item.experimentId,
                );
                const protocol = store.protocols.find(
                  (candidate) => candidate.id === item.protocolId,
                );
                const task = taskForRecord(item.id);
                return (
                  <div className="record-row" key={item.id}>
                    <input
                      aria-label={`选择 ${item.title}`}
                      type="checkbox"
                      checked={selectedIds.has(item.id)}
                      onChange={(event) =>
                        toggleRecords([item.id], event.target.checked)
                      }
                    />
                    <button onClick={() => openRecord(item.id)}>
                      <i>▱</i>
                      <div>
                        <b>{item.title}</b>
                        <p>
                          {experiment?.code} · {experiment?.title} ·{" "}
                          {item.protocolName ||
                            protocol?.name ||
                            item.protocolId}
                        </p>
                      </div>
                      <time>{task?.start.slice(11, 16)}</time>
                      <small>更新于 {item.updated} →</small>
                    </button>
                  </div>
                );
              })}
            </section>
          );
        })}
        {store.records.length === 0 && (
          <div className="empty">暂无实验记录。</div>
        )}
        {store.records.length > 0 && visibleRecords.length === 0 && (
          <div className="empty">所选日期范围内暂无实验记录。</div>
        )}
      </div>
      {exportPreview && (
        <div className="export-preview">
          <div className="export-preview-actions">
            <button
              className="secondary"
              onClick={() => setExportPreview(undefined)}
              disabled={pdfBusy || printMode}
            >
              返回选择
            </button>
            <span>
              {exportPreview.manifest.recordCount} 条 · 校验值{" "}
              {exportPreview.manifest.contentSha256.slice(0, 12)}…
            </span>
            <button
              className="secondary"
              disabled={pdfBusy || printMode}
              onClick={() => void printExport()}
            >
              打印 / 保存 PDF
            </button>
            <button
              className="primary"
              disabled={pdfBusy || printMode}
              onClick={() => void lowMemoryExport()}
            >
              {pdfBusy ? "正在导出…" : "低内存 PDF"}
            </button>
            {pdfBusy && (
              <button
                className="secondary"
                disabled={pdfFinishing}
                onClick={() => pdfController.current?.abort()}
              >
                取消
              </button>
            )}
          </div>
          <div className="export-job-status" role="status">
            <p>
              大量图片请选择“低内存
              PDF”：逐页保存为图像，文字不可选中复制。需要可复制文字时，可使用系统打印（最多
              8 张图片）。
            </p>
            {pdfProgress && <p>{pdfProgress}</p>}
            {exportError && (
              <p className="form-error" role="alert">
                {exportError}
              </p>
            )}
          </div>
          {!pdfBusy && (
            <article className="export-document">
              <header className="export-cover">
                <p>LABFLOW ELECTRONIC LAB NOTEBOOK</p>
                <h1>电子实验记录</h1>
                <dl>
                  <div>
                    <dt>日期范围</dt>
                    <dd>
                      {recordDate(exportPreview.records[0].id)} —{" "}
                      {recordDate(exportPreview.records.at(-1)!.id)}
                    </dd>
                  </div>
                  <div>
                    <dt>记录数量</dt>
                    <dd>{exportPreview.records.length}</dd>
                  </div>
                  <div>
                    <dt>生成时间</dt>
                    <dd>{new Date().toLocaleString("zh-CN")}</dd>
                  </div>
                  <div>
                    <dt>内容校验</dt>
                    <dd>{exportPreview.manifest.contentSha256}</dd>
                  </div>
                </dl>
              </header>
              {exportPreview.records.map((item, index) => {
                const task = taskForRecord(item.id);
                const experiment = store.experiments.find(
                  (candidate) => candidate.id === item.experimentId,
                );
                const protocol = store.protocols.find(
                  (candidate) => candidate.id === item.protocolId,
                );
                const previousDate =
                  index > 0
                    ? recordDate(exportPreview.records[index - 1].id)
                    : "";
                const date = recordDate(item.id);
                return (
                  <section className="export-record" key={item.id}>
                    {date !== previousDate && (
                      <h2 className="export-date">{date}</h2>
                    )}
                    <header>
                      <div>
                        <h3>{item.title}</h3>
                        <p>
                          {experiment?.code} · {experiment?.title}
                        </p>
                      </div>
                      <time>{task?.start.slice(11, 16)}</time>
                    </header>
                    <dl className="export-meta">
                      <div>
                        <dt>Protocol</dt>
                        <dd>
                          {item.protocolName ||
                            protocol?.name ||
                            item.protocolId}{" "}
                          · v{item.protocolVersion || "snapshot"}
                        </dd>
                      </div>
                      <div>
                        <dt>Record ID</dt>
                        <dd>{item.id}</dd>
                      </div>
                    </dl>
                    <section>
                      <h4>实验正文</h4>
                      <RecordBody
                        attachments={item.attachments}
                        className="export-body"
                        content={
                          item.renderedContent || item.notes || "暂无正文。"
                        }
                        eager={printMode}
                      />
                    </section>
                    {item.analysisSections?.map((analysis) => (
                      <section key={analysis.id}>
                        <h4>{analysis.title}</h4>
                        <p className="export-body">{analysis.text}</p>
                      </section>
                    ))}
                    <section className="export-samples">
                      <h4>样本</h4>
                      <p>
                        输入：
                        {item.inputs
                          .map(
                            (id) =>
                              store.samples.find((sample) => sample.id === id)
                                ?.code || id,
                          )
                          .join("、") || "无"}
                      </p>
                      <p>
                        输出：
                        {item.outputs
                          .map(
                            (id) =>
                              store.samples.find((sample) => sample.id === id)
                                ?.code || id,
                          )
                          .join("、") || "无"}
                      </p>
                    </section>
                    {!!item.results?.length && (
                      <section>
                        <h4>Results</h4>
                        {item.results.map((result) => (
                          <p key={result.id}>
                            {result.type} · {JSON.stringify(result.data)}
                          </p>
                        ))}
                      </section>
                    )}
                    {!!item.attachments?.length && (
                      <section>
                        <h4>附件目录</h4>
                        {item.attachments.map((attachment) => (
                          <p key={attachment.id}>
                            {attachment.fileName} · {attachment.relativePath}
                          </p>
                        ))}
                      </section>
                    )}
                  </section>
                );
              })}
            </article>
          )}
        </div>
      )}
      {record && !exportPreview && (
        <div className="overlay record-overlay">
          <section className="record-panel">
            <header className="record-header">
              <button className="back" onClick={closeRecordView}>
                ←
              </button>
              <div>
                <h1>{record.title}</h1>
                <p>本地实验记录 · 更新于 {record.updated}</p>
              </div>
              <button
                className="danger"
                onClick={() => {
                  setDeleteError("");
                  setDeleteConfirmOpen(true);
                }}
              >
                删除记录
              </button>
              <button className="secondary" onClick={closeRecordView}>
                完成
              </button>
            </header>
            {deleteError && (
              <p className="form-error record-delete-error">{deleteError}</p>
            )}
            <div className="record-content">
              <article>
                <section className="record-section">
                  <div className="section-title">
                    <div>
                      <i>01</i>
                      <h2>样本</h2>
                    </div>
                  </div>
                  <p>
                    输入样本 {record.inputs.length} 个，输出样本{" "}
                    {record.outputs.length} 个。
                  </p>
                  <div className="protocol-tools">
                    {record.inputs.map((id) => (
                      <span className="sample-row" key={`input-${id}`}>
                        输入：
                        {store.samples.find((sample) => sample.id === id)
                          ?.code || id}
                      </span>
                    ))}
                    {record.outputs.map((id) => (
                      <span className="sample-row output" key={`output-${id}`}>
                        输出：
                        {store.samples.find((sample) => sample.id === id)
                          ?.code || id}
                      </span>
                    ))}
                    {record.results?.map((result) => (
                      <span className="sample-row" key={result.id}>
                        Result：{result.type} ·{" "}
                        {String(result.data.status || "pending")}
                      </span>
                    ))}
                  </div>
                </section>
                <section className="record-section">
                  <div className="section-title">
                    <div>
                      <i>02</i>
                      <h2>实验正文</h2>
                    </div>
                    {!editingBody && (
                      <button className="secondary" onClick={beginBodyEdit}>
                        修改正文
                      </button>
                    )}
                  </div>
                  {editingBody ? (
                    <div className="record-body-editor">
                      <p className="muted">
                        仅修改此 Record，不影响 Protocol 模板或其他
                        Record。插入图片会立即保存当前正文。
                      </p>
                      <textarea
                        aria-label="实验正文"
                        ref={bodyTextareaRef}
                        value={bodyDraft}
                        onChange={(event) => setBodyDraft(event.target.value)}
                      />
                      <div className="record-image-insert-row">
                        <button
                          className="secondary"
                          disabled={savingBody || insertingImage}
                          onClick={() => void addImageToBody()}
                          type="button"
                        >
                          {insertingImage ? "处理图片中…" : "在光标处插入图片"}
                        </button>
                        <small>
                          支持 PNG、JPEG、WebP、TIFF；大图会保留原图并生成预览。
                        </small>
                      </div>
                      <div className="record-body-live-preview">
                        <b>正文预览</b>
                        <RecordBody
                          attachments={record.attachments}
                          content={bodyDraft || "暂无正文。"}
                        />
                      </div>
                      {bodyError && <p className="form-error">{bodyError}</p>}
                      <div className="record-body-actions">
                        <button
                          className="secondary"
                          disabled={savingBody}
                          onClick={() => {
                            setEditingBody(false);
                            setBodyError("");
                          }}
                        >
                          取消
                        </button>
                        <button
                          className="primary"
                          disabled={savingBody}
                          onClick={() => void saveBody()}
                        >
                          {savingBody ? "保存中…" : "保存正文"}
                        </button>
                      </div>
                    </div>
                  ) : (
                    <RecordBody
                      attachments={record.attachments}
                      content={
                        record.renderedContent || record.notes || "暂无正文。"
                      }
                    />
                  )}
                </section>
                {!!record.analysisSections?.length && (
                  <section className="record-section record-analysis-sections">
                    <div className="section-title">
                      <div>
                        <i>03</i>
                        <h2>qPCR 分析结果</h2>
                      </div>
                    </div>
                    {record.analysisSections.map((section) => (
                      <article key={section.id}>
                        <h3>{section.title}</h3>
                        <p style={{ whiteSpace: "pre-wrap" }}>{section.text}</p>
                      </article>
                    ))}
                  </section>
                )}
                {recordTerminalAssay && (
                  <TerminalAssayWorkspace
                    record={record}
                    samples={store.samples}
                    definition={recordTerminalAssay}
                    changed={changed}
                  />
                )}
              </article>
            </div>
          </section>
        </div>
      )}
      {deleteConfirmOpen && record && (
        <div className="overlay centered record-confirm-overlay">
          <section
            className="modal record-delete-confirm"
            role="dialog"
            aria-modal="true"
            aria-labelledby="record-delete-title"
          >
            <h2 id="record-delete-title">删除实验记录？</h2>
            <p>
              确定删除“{record.title}”吗？删除后，该 Task 会恢复为计划中。若输出
              Sample 已被下游使用，系统会阻止删除。
            </p>
            {deleteError && <p className="form-error">{deleteError}</p>}
            <div className="record-delete-actions">
              <button
                className="secondary"
                disabled={deleting}
                onClick={() => {
                  setDeleteConfirmOpen(false);
                  setDeleteError("");
                }}
              >
                取消
              </button>
              <button
                className="danger"
                disabled={deleting}
                onClick={() => void removeRecord()}
              >
                {deleting ? "删除中…" : "确认删除"}
              </button>
            </div>
          </section>
        </div>
      )}
    </section>
  );
}

function TaskModal({
  task,
  experiments,
  tasks,
  done,
  cancel,
}: {
  task: Task;
  experiments: Experiment[];
  tasks: Task[];
  done: () => void;
  cancel: () => void;
}) {
  const initiallyEligibleParentIds = new Set(
    eligibleParentTaskOptions(
      tasks,
      task.experimentId,
      task.id,
      task.start.slice(0, 16),
    ).map((candidate) => candidate.id),
  );
  const initialParentTaskIds = (task.parentTaskIds || []).filter((id) =>
    initiallyEligibleParentIds.has(id),
  );
  const [title, setTitle] = useState(task.title),
    [experimentId, setExperimentId] = useState(task.experimentId),
    [newExperiment, setNewExperiment] = useState(false),
    [newExperimentName, setNewExperimentName] = useState(""),
    [parentTaskIds, setParentTaskIds] = useState(initialParentTaskIds),
    [prunedParentCount, setPrunedParentCount] = useState(
      (task.parentTaskIds || []).length - initialParentTaskIds.length,
    ),
    [start, setStart] = useState(task.start.slice(0, 16)),
    [end, setEnd] = useState(task.end.slice(0, 16)),
    [error, setError] = useState("");
  const editing = Boolean(task.title);
  const parentTaskOptions = eligibleParentTaskOptions(
    tasks,
    experimentId,
    task.id,
    start,
  );
  const changeStart = (nextStart: string) => {
    const eligibleIds = new Set(
      eligibleParentTaskOptions(tasks, experimentId, task.id, nextStart).map(
        (candidate) => candidate.id,
      ),
    );
    const retained = parentTaskIds.filter((id) => eligibleIds.has(id));
    setPrunedParentCount(
      (current) => current + parentTaskIds.length - retained.length,
    );
    setParentTaskIds(retained);
    setStart(nextStart);
  };
  const save = async (event: React.FormEvent) => {
    event.preventDefault();
    if (!title.trim()) return setError("Task 名称是必填项。");
    if (end <= start) return setError("结束时间必须晚于开始时间。");
    if (!newExperiment && !experimentId)
      return setError("请选择归属 Experiment，或在此处新建一个。");
    if (newExperiment && !newExperimentName.trim())
      return setError("Experiment 名称是必填项。");
    try {
      await saveTask(
        {
          id: task.id,
          title: title.trim(),
          experimentId: newExperiment ? undefined : experimentId,
          newExperimentId: uid("exp"),
          newExperimentCode: `EXP${Date.now().toString().slice(-6)}`,
          start,
          end,
          updatedAt: new Date().toISOString(),
        },
        newExperiment ? newExperimentName.trim() : undefined,
        newExperiment ? [] : parentTaskIds,
      );
      done();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  };
  const remove = async () => {
    try {
      await deleteTask(task.id);
      done();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  };
  return (
    <div className="overlay centered">
      <form className="modal task-form task-modal" onSubmit={save}>
        <button className="close" type="button" onClick={cancel}>
          ×
        </button>
        <div className="task-form-scroll">
          <p className="eyebrow">CALENDAR TASK</p>
          <h2>{editing ? "编辑任务" : "新建任务"}</h2>
          <p>任务将直接保存到本机 LabFlow 数据库。</p>
          <label>
            任务名称
            <input
              autoFocus
              value={title}
              onChange={(e) => setTitle(e.target.value)}
            />
          </label>
          <label>
            归属 Experiment
            <select
              disabled={newExperiment}
              value={experimentId}
              onChange={(e) => {
                setExperimentId(e.target.value);
                setParentTaskIds([]);
                setPrunedParentCount(0);
              }}
            >
              <option value="">选择已有 Experiment</option>
              {experiments.map((e) => (
                <option value={e.id} key={e.id}>
                  {e.title}
                </option>
              ))}
            </select>
          </label>
          <button
            className="link-button"
            type="button"
            onClick={() => {
              setNewExperiment(!newExperiment);
              setParentTaskIds([]);
              setPrunedParentCount(0);
              setError("");
            }}
          >
            {newExperiment ? "使用已有 Experiment" : "＋ 在此新建 Experiment"}
          </button>
          {newExperiment && (
            <label>
              Experiment 名称
              <input
                value={newExperimentName}
                onChange={(e) => setNewExperimentName(e.target.value)}
              />
            </label>
          )}
          <div className="time-grid">
            <label>
              开始时间
              <input
                type="datetime-local"
                step="3600"
                value={start}
                onChange={(e) => changeStart(e.target.value)}
              />
            </label>
            <label>
              结束时间
              <input
                type="datetime-local"
                step="3600"
                value={end}
                onChange={(e) => setEnd(e.target.value)}
              />
            </label>
          </div>
          {!newExperiment && experimentId && (
            <fieldset className="task-dependencies">
              <legend>上级 Task（可多选）</legend>
              <p>
                只显示当前 Task 开始时间之前的同 Experiment
                Task，并按时间顺序排列。
              </p>
              {prunedParentCount > 0 && (
                <p className="task-dependency-warning">
                  已移除 {prunedParentCount} 个不再早于当前 Task 的上级关系。
                </p>
              )}
              <div className="task-dependency-options">
                {parentTaskOptions.map((candidate) => (
                  <label key={candidate.id}>
                    <input
                      type="checkbox"
                      checked={parentTaskIds.includes(candidate.id)}
                      onChange={(event) =>
                        setParentTaskIds((current) =>
                          event.target.checked
                            ? [...current, candidate.id]
                            : current.filter((id) => id !== candidate.id),
                        )
                      }
                    />
                    <span>
                      <b>{candidate.title}</b>
                      <small>
                        {dayLabel(candidate.start)} ·{" "}
                        {formatTime(candidate.start)}
                      </small>
                    </span>
                  </label>
                ))}
              </div>
              {parentTaskOptions.length === 0 && (
                <p>当前 Experiment 暂无符合时间条件的上级 Task。</p>
              )}
            </fieldset>
          )}
          {error && <p className="form-error">{error}</p>}
        </div>
        <footer>
          <button type="button" className="secondary" onClick={cancel}>
            取消
          </button>
          {editing && (
            <button
              type="button"
              className="secondary"
              onClick={() => void remove()}
            >
              删除
            </button>
          )}
          <button className="primary">保存任务</button>
        </footer>
      </form>
    </div>
  );
}
