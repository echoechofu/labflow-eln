import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import "./App.css";
import "./task-modal.css";
import type {
  Experiment,
  NavPage,
  Protocol,
  RecordAttachment,
  Task,
} from "./domain";
import { dayLabel, formatTime } from "./domain";
import {
  createExportManifest,
  beginRecordPdf,
  appendRecordPdfPage,
  finishRecordPdf,
  cancelRecordPdf,
  beginRecordBundlePdf,
  appendRecordBundlePdfPage,
  finishRecordBundlePdf,
  cancelRecordBundlePdf,
  recordImagePreviewUrl,
  chooseRecordImage,
  chooseRecordFiles,
  chooseWorkspaceBackup,
  deleteProtocol,
  deleteRecord,
  deleteTask,
  loadStore,
  markExportPrintRequested,
  exportWorkspaceBackup,
  insertRecordImage,
  insertRecordFiles,
  openRecordAttachment,
  saveRecordAttachmentAs,
  restoreWorkspaceBackup,
  saveExperimentGraphPng,
  saveTask,
  uid,
  updateRecordBody,
  type Store,
  type WorkspaceBackupSummary,
} from "./repository";
import {
  experimentGraphPngFileName,
  renderExperimentGraphPng,
} from "./experimentGraphPng";
import { RecordBody } from "./RecordBody";
import { TaskDrawer } from "./RecordCreationDrawer";
import { recordPdfBlocks, renderRecordPdf } from "./recordPdf";
import {
  attachmentLabelFromPath,
  insertFileReferences,
  imageCaptionFromPath,
  insertImageReference,
  parseRecordBody,
} from "./recordBodyFormat";
import {
  buildTaskGraph,
  TASK_GRAPH_NODE_HEIGHT,
  TASK_GRAPH_NODE_WIDTH,
} from "./taskGraph";
import { eligibleParentTaskOptions } from "./taskInputs";
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

function ExperimentsPage({
  store,
  openTask,
}: {
  store: Store;
  openTask: (task: Task) => void;
}) {
  const [selectedExperimentId, setSelectedExperimentId] = useState<string>();
  const [exportingGraph, setExportingGraph] = useState(false);
  const [graphExportMessage, setGraphExportMessage] = useState("");
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
  const exportGraph = async () => {
    if (!selectedExperiment || experimentTasks.length === 0) return;
    setExportingGraph(true);
    setGraphExportMessage("");
    try {
      const subtitleByTaskId = Object.fromEntries(
        experimentTasks.map((task) => {
          const record =
            store.records.find((item) => item.id === task.recordId) ||
            store.records.find((item) => item.taskId === task.id);
          const protocol = store.protocols.find(
            (item) => item.id === record?.protocolId,
          );
          return [
            task.id,
            record
              ? record.protocolName || protocol?.name || "已有 Record"
              : "尚无 Record",
          ];
        }),
      );
      const png = await renderExperimentGraphPng({
        experiment: selectedExperiment,
        graph,
        subtitleByTaskId,
        dateRange: firstDate
          ? `${firstDate}${lastDate !== firstDate ? ` — ${lastDate}` : ""}`
          : undefined,
      });
      const destination = await saveExperimentGraphPng(
        experimentGraphPngFileName(selectedExperiment),
        png,
      );
      if (destination) setGraphExportMessage(`PNG 已保存：${destination}`);
    } catch (cause) {
      setGraphExportMessage(
        cause instanceof Error ? cause.message : String(cause),
      );
    } finally {
      setExportingGraph(false);
    }
  };
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
        <div className="experiment-detail-actions">
          <span className="readonly-badge">只读 Task 网络</span>
          <button
            className="secondary"
            disabled={experimentTasks.length === 0 || exportingGraph}
            onClick={() => void exportGraph()}
          >
            {exportingGraph ? "正在生成…" : "导出 PNG"}
          </button>
        </div>
      </header>
      {graphExportMessage && (
        <p className="graph-export-message" role="status">
          {graphExportMessage}
        </p>
      )}
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
  const [viewing, setViewing] = useState<Protocol>();
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
                <button onClick={() => setViewing(protocol)}>查看</button>
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
      {viewing && (
        <ProtocolViewer
          protocol={viewing}
          close={() => setViewing(undefined)}
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

const protocolFieldKindLabel: Record<string, string> = {
  text: "文字",
  number: "数字",
  select: "下拉选项",
  samples: "Sample",
  plate_layout: "孔板布局",
  condition_groups: "条件分组",
};

const protocolOutputModeLabel: Record<string, string> = {
  one: "产生 1 个新 Sample",
  count: "产生指定数量的新 Sample",
  per_input: "每个输入产生 1 个新 Sample",
  per_input_count: "每个输入产生多个相同条件的 Sample",
  per_input_conditions: "按实验条件产生多个 Sample",
  per_input_types: "每个输入产生多种类型的 Sample",
  same_sample: "原 Sample 继续",
  plate_or_dish: "按孔板或培养皿分配",
  plate_wells: "按孔位分配",
  none: "仅检测，不产生 Sample",
};

function ProtocolViewer({
  protocol,
  close,
}: {
  protocol: Protocol;
  close: () => void;
}) {
  const execution = protocol.execution;
  const templates = Object.entries(protocol.templateVariants || {});
  return (
    <div className="overlay centered protocol-view-overlay">
      <section
        className="modal protocol-viewer"
        role="dialog"
        aria-modal="true"
        aria-labelledby="protocol-view-title"
      >
        <button className="close" aria-label="关闭" onClick={close}>
          ×
        </button>
        <header className="protocol-viewer-header">
          <div>
            <p className="eyebrow">CURRENT PROTOCOL</p>
            <h2 id="protocol-view-title">{protocol.name}</h2>
            {protocol.description && <p>{protocol.description}</p>}
          </div>
          <span>当前启用 v{protocol.version}</span>
        </header>

        <div className="protocol-viewer-meta">
          <span>分类：{protocol.category}</span>
          <span>
            来源：
            {protocol.activeVersionOrigin === "user" ||
            protocol.origin === "user"
              ? "自定义"
              : "内置"}
          </span>
        </div>

        <section className="protocol-viewer-section">
          <h3>实验步骤</h3>
          {protocol.blocks.length ? (
            <ol>
              {protocol.blocks.map((block) => (
                <li key={block}>{block}</li>
              ))}
            </ol>
          ) : (
            <p className="muted">未设置实验步骤。</p>
          )}
        </section>

        <section className="protocol-viewer-section">
          <h3>Sample Flow</h3>
          {execution ? (
            <dl>
              <div>
                <dt>输入类型</dt>
                <dd>{execution.inputTypes?.join("、") || "不限"}</dd>
              </div>
              <div>
                <dt>输入数量</dt>
                <dd>
                  {execution.inputCardinality === "many" ? "可多个" : "1 个"}
                </dd>
              </div>
              <div>
                <dt>完成后</dt>
                <dd>
                  {protocolOutputModeLabel[execution.outputMode] ||
                    execution.outputMode}
                </dd>
              </div>
              <div>
                <dt>输出类型</dt>
                <dd>
                  {execution.outputRules?.length
                    ? execution.outputRules
                        .map((rule) => `${rule.sampleType} × ${rule.count}`)
                        .join("、")
                    : execution.outputType || "与输入相同 / 不适用"}
                </dd>
              </div>
              <div>
                <dt>输入 Sample</dt>
                <dd>
                  {execution.consumptionPolicy === "consume"
                    ? "视为已转化或消耗"
                    : "保留"}
                </dd>
              </div>
              {execution.conditionAllocation?.containerMode && (
                <div>
                  <dt>条件分配</dt>
                  <dd>
                    {
                      {
                        independent: "不绑定容器位置",
                        plate: "按孔板孔位",
                        dish: "按培养皿",
                      }[execution.conditionAllocation.containerMode]
                    }
                  </dd>
                </div>
              )}
            </dl>
          ) : (
            <p className="muted">未设置 Sample Flow。</p>
          )}
        </section>

        <section className="protocol-viewer-section">
          <h3>Record 字段</h3>
          {protocol.fields?.length ? (
            <div className="protocol-viewer-fields">
              {protocol.fields.map((field) => (
                <article key={field.key}>
                  <div>
                    <b>{field.label}</b>
                    {field.required && <em>必填</em>}
                  </div>
                  <code>{`{{${field.key}}}`}</code>
                  <small>
                    {protocolFieldKindLabel[field.kind] || field.kind}
                    {field.unit ? ` · 单位 ${field.unit}` : ""}
                    {field.defaultValue
                      ? ` · 默认值 ${field.defaultValue}`
                      : ""}
                  </small>
                </article>
              ))}
            </div>
          ) : (
            <p className="muted">未设置自定义字段。</p>
          )}
        </section>

        <section className="protocol-viewer-section">
          <h3>Record 正文</h3>
          {templates.length ? (
            <div className="protocol-viewer-templates">
              {templates.map(([name, body]) => (
                <article key={name}>
                  <b>{name}</b>
                  <pre>{body}</pre>
                </article>
              ))}
            </div>
          ) : protocol.template ? (
            <pre>{protocol.template}</pre>
          ) : (
            <p className="muted">未设置 Record 正文。</p>
          )}
        </section>
      </section>
    </div>
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
  const [bundleBusy, setBundleBusy] = useState(false);
  const [bundleProgress, setBundleProgress] = useState("");
  const [printMode, setPrintMode] = useState(false);
  const pdfController = useRef<AbortController | undefined>(undefined);
  const bundleController = useRef<AbortController | undefined>(undefined);
  useEffect(
    () => () => {
      pdfController.current?.abort();
      bundleController.current?.abort();
    },
    [],
  );
  const [deleteError, setDeleteError] = useState("");
  const [deleteConfirmOpen, setDeleteConfirmOpen] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [editingBody, setEditingBody] = useState(false);
  const [bodyDraft, setBodyDraft] = useState("");
  const [bodyError, setBodyError] = useState("");
  const [savingBody, setSavingBody] = useState(false);
  const [insertingImage, setInsertingImage] = useState(false);
  const [insertingFile, setInsertingFile] = useState(false);
  const [attachmentMessage, setAttachmentMessage] = useState("");
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
    setInsertingFile(false);
    setAttachmentMessage("");
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
      setBundleProgress("");
      setExportError("");
    } catch (reason) {
      setExportError(reason instanceof Error ? reason.message : String(reason));
    }
  };
  const printExport = async () => {
    if (!exportPreview || printMode || pdfController.current || bundleBusy) return;
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
    if (!exportPreview || pdfController.current || printMode || bundleBusy) return;
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
  const addFilesToBody = async () => {
    if (!record) return;
    try {
      const sourcePaths = await chooseRecordFiles();
      if (!sourcePaths.length) return;
      const files = sourcePaths.map((sourcePath) => ({
        id: uid("attachment"),
        sourcePath,
        label: attachmentLabelFromPath(sourcePath),
      }));
      const selection =
        bodyTextareaRef.current?.selectionStart ?? bodyDraft.length;
      const inserted = insertFileReferences(
        bodyDraft,
        selection,
        files.map(({ id, label }) => ({ id, label })),
      );
      setInsertingFile(true);
      setBodyError("");
      await insertRecordFiles({
        recordId: record.id,
        files: files.map(({ id, sourcePath }) => ({ id, sourcePath })),
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
      setInsertingFile(false);
    }
  };
  const openAttachment = async (attachment: RecordAttachment) => {
    if (!record) return;
    setAttachmentMessage("");
    try {
      await openRecordAttachment(record.id, attachment.id);
    } catch (reason) {
      setAttachmentMessage(reason instanceof Error ? reason.message : String(reason));
    }
  };
  const saveAttachment = async (attachment: RecordAttachment) => {
    if (!record) return;
    setAttachmentMessage("");
    try {
      const destination = await saveRecordAttachmentAs(record.id, attachment);
      if (destination) setAttachmentMessage(`附件已保存到：${destination}`);
    } catch (reason) {
      setAttachmentMessage(reason instanceof Error ? reason.message : String(reason));
    }
  };
  const exportBundle = async () => {
    if (!exportPreview || bundleController.current || pdfBusy || printMode) return;
    const controller = new AbortController();
    bundleController.current = controller;
    setBundleBusy(true);
    setBundleProgress("请选择保存位置…");
    setExportError("");
    let job: string | undefined;
    try {
      job = await beginRecordBundlePdf(exportPreview.records);
      controller.signal.throwIfAborted();
      if (!job) {
        setBundleProgress("");
        return;
      }
      setBundleProgress("正在逐页生成低内存 PDF…");
      await renderRecordPdf(
        recordPdfBlocks(
          exportPreview.records,
          exportPreview.store,
          exportPreview.manifest,
        ),
        {
          signal: controller.signal,
          imageUrl: recordImagePreviewUrl,
          writePage: (jpeg, sequence) =>
            appendRecordBundlePdfPage(job!, sequence, jpeg),
          progress: (pages, images) =>
            setBundleProgress(`已写入 ${pages} 页 · 已处理 ${images} 张图片`),
        },
      );
      controller.signal.throwIfAborted();
      setBundleProgress("正在归档 PDF 和附件…");
      const result = await finishRecordBundlePdf(job);
      job = undefined;
      setBundleProgress(
        `已导出 ${result.recordCount} 条 Record 的低内存 PDF 和 ${result.attachmentCount} 个附件：${result.path}`,
      );
      try {
        await markExportPrintRequested(exportPreview.manifest.id);
      } catch (reason) {
        setExportError(`ZIP 已保存，但导出审计状态更新失败：${String(reason)}`);
      }
    } catch (reason) {
      if (controller.signal.aborted)
        setBundleProgress("导出已取消，未完成文件已清理。");
      else {
        setBundleProgress("");
        setExportError(
          `ZIP 导出失败：${reason instanceof Error ? reason.message : String(reason)}`,
        );
      }
    } finally {
      if (job) {
        try {
          await cancelRecordBundlePdf(job);
        } catch {
          setExportError("ZIP PDF 临时文件清理失败，请重启 LabFlow 后检查保存目录。");
        }
      }
      bundleController.current = undefined;
      setBundleBusy(false);
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
              disabled={pdfBusy || printMode || bundleBusy}
            >
              返回选择
            </button>
            <span>
              {exportPreview.manifest.recordCount} 条 · 校验值{" "}
              {exportPreview.manifest.contentSha256.slice(0, 12)}…
            </span>
            <button
              className="secondary"
              disabled={pdfBusy || printMode || bundleBusy}
              onClick={() => void printExport()}
            >
              打印 / 保存 PDF
            </button>
            <button
              className="primary"
              disabled={pdfBusy || printMode || bundleBusy}
              onClick={() => void lowMemoryExport()}
            >
              {pdfBusy ? "正在导出…" : "低内存 PDF"}
            </button>
            <button
              className="primary"
              disabled={pdfBusy || printMode || bundleBusy}
              onClick={() => void exportBundle()}
            >
              {bundleBusy ? "正在打包…" : "低内存 PDF＋附件 ZIP"}
            </button>
            {(pdfBusy || bundleBusy) && (
              <button
                className="secondary"
                disabled={pdfFinishing}
                onClick={() => {
                  if (pdfBusy) pdfController.current?.abort();
                  else bundleController.current?.abort();
                }}
              >
                取消
              </button>
            )}
          </div>
          <div className="export-job-status" role="status">
            <p>
              大量图片请选择“低内存
              PDF”：逐页保存为图像，文字不可选中复制。需要可复制文字时，可使用系统打印（最多
              8 张图片）。“低内存 PDF＋附件 ZIP”会将同一份逐页 PDF 与所选 Records 的全部原始附件放入一个 ZIP。
            </p>
            {pdfProgress && <p>{pdfProgress}</p>}
            {bundleProgress && <p>{bundleProgress}</p>}
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
            {attachmentMessage && (
              <p className="record-attachment-message">{attachmentMessage}</p>
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
                        Record。插入图片或文件会立即保存当前正文。
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
                          disabled={savingBody || insertingImage || insertingFile}
                          onClick={() => void addImageToBody()}
                          type="button"
                        >
                          {insertingImage ? "处理图片中…" : "在光标处插入图片"}
                        </button>
                        <small>
                          支持 PNG、JPEG、WebP、TIFF；大图会保留原图并生成预览。
                        </small>
                      </div>
                      <div className="record-image-insert-row">
                        <button
                          className="secondary"
                          disabled={savingBody || insertingImage || insertingFile}
                          onClick={() => void addFilesToBody()}
                          type="button"
                        >
                          {insertingFile ? "归档文件中…" : "在光标处插入文件"}
                        </button>
                        <small>
                          可一次选择多个任意类型文件；原文件保存在 LabFlow 用户数据目录，不写入 SQLite，也不生成预览。
                        </small>
                      </div>
                      <div className="record-body-live-preview">
                        <b>正文预览</b>
                        <RecordBody
                          attachments={record.attachments}
                          content={bodyDraft || "暂无正文。"}
                          onOpenAttachment={openAttachment}
                          onSaveAttachment={saveAttachment}
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
                      onOpenAttachment={openAttachment}
                      onSaveAttachment={saveAttachment}
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
