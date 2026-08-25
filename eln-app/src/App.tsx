import { useEffect, useMemo, useState } from "react";
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
  deleteTask,
  loadStore,
  markExportPrintRequested,
  saveTask,
  startTaskRecord,
  uid,
  updateTaskStatus,
  type Store,
} from "./repository";
import {
  buildTaskGraph,
  TASK_GRAPH_NODE_HEIGHT,
  TASK_GRAPH_NODE_WIDTH,
} from "./taskGraph";
import TerminalAssayWorkspace from "./TerminalAssayWorkspace";

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
  const load = () => void loadStore().then(setStore);
  useEffect(load, []);
  if (!store) return <main className="page">正在读取本地数据…</main>;
  const nav = [
    ["calendar", "◫", "日历"],
    ["experiments", "◈", "实验"],
    ["protocols", "▤", "Protocols"],
    ["records", "▧", "Records"],
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
        <div className="sidebar-foot">
          <p>
            <span>本地数据</span>
            <b>8%</b>
          </p>
          <div className="storage">
            <i />
          </div>
        </div>
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
      {page === "protocols" && <ProtocolsPage protocols={store.protocols} />}
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
      {selectedTask && (
        <TaskDrawer
          task={selectedTask}
          experiment={store.experiments.find(
            (item) => item.id === selectedTask.experimentId,
          )}
          samples={store.samples}
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
        <div className="seg">
          <button className="selected">周</button>
          <button disabled>月</button>
        </div>
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
  protocols,
  tasks,
  records,
  close,
  edit,
  openRecord,
  changed,
}: {
  task: Task;
  experiment?: Experiment;
  samples: Store["samples"];
  protocols: Store["protocols"];
  tasks: Store["tasks"];
  records: Store["records"];
  close: () => void;
  edit: () => void;
  openRecord: () => void;
  changed: () => void;
}) {
  const [error, setError] = useState("");
  const [choosingProtocol, setChoosingProtocol] = useState(false);
  const [protocol, setProtocol] = useState<Protocol>();
  const [values, setValues] = useState<Record<string, string>>({});
  const [sourceTaskIds, setSourceTaskIds] = useState<string[]>([]);
  const [inputSampleIds, setInputSampleIds] = useState<string[]>([]);
  const [plateGroups, setPlateGroups] = useState<PlateTreatmentGroup[]>([
    { factor: "", duration: "", wellCount: 1 },
  ]);
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
  const usesParentTaskOutputs =
    protocol?.execution?.inputSource === "parent_task_outputs";
  const upstreamTasks = tasks.filter((item) =>
    (task.parentTaskIds || []).includes(item.id),
  );
  const eligibleInputSamples = samples.filter((sample) => {
    if (!usesParentTaskOutputs) return false;
    if (sample.consumed) return false;
    if (
      !(protocol?.execution?.inputTypes ?? [])
        .map(normalizeSampleType)
        .includes(normalizeSampleType(sample.type))
    )
      return false;
    return sourceTaskIds.some((sourceTaskId) => {
      const sourceTask = tasks.find((item) => item.id === sourceTaskId);
      const sourceRecord = records.find(
        (record) => record.id === sourceTask?.recordId,
      );
      return sourceRecord?.outputs.includes(sample.id);
    });
  });
  const selectProtocol = (item: Protocol) => {
    setProtocol(item);
    setError("");
    setInputSampleIds([]);
    setSourceTaskIds([]);
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
    if (usesParentTaskOutputs && sourceTaskIds.length === 0)
      return setError("请先选择至少一个上级 Task。");
    if (usesParentTaskOutputs && inputSampleIds.length === 0)
      return setError("请从上级 Task 的输出中选择至少一个 Sample。");
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
        usesParentTaskOutputs
          ? inputSampleIds
          : values.input_sample
            ? [values.input_sample]
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
          onClick={task.recordId ? openRecord : () => setChoosingProtocol(true)}
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
            <div className="modal">
              <button
                className="close"
                onClick={() => {
                  setChoosingProtocol(false);
                  setProtocol(undefined);
                }}
              >
                ×
              </button>
              {!protocol ? (
                protocols.map((item) => (
                  <button
                    className="picker"
                    onClick={() => selectProtocol(item)}
                    key={item.id}
                  >
                    <div>
                      <b>{item.name}</b>
                      <small>{item.category}</small>
                    </div>
                    →
                  </button>
                ))
              ) : (
                <>
                  <h2>{protocol.name}</h2>
                  {usesParentTaskOutputs && (
                    <fieldset className="protocol-inputs">
                      <legend>1. 选择来源上级 Task</legend>
                      {upstreamTasks.map((sourceTask) => (
                        <label key={sourceTask.id}>
                          <input
                            type="checkbox"
                            checked={sourceTaskIds.includes(sourceTask.id)}
                            onChange={(event) => {
                              setSourceTaskIds((current) =>
                                event.target.checked
                                  ? [...current, sourceTask.id]
                                  : current.filter(
                                      (id) => id !== sourceTask.id,
                                    ),
                              );
                              setInputSampleIds([]);
                              setError("");
                            }}
                          />
                          <span>
                            <b>{sourceTask.title}</b>
                            <small>
                              {sourceTask.recordId
                                ? "已有 Record"
                                : "尚未产生 Record"}
                            </small>
                          </span>
                        </label>
                      ))}
                      {upstreamTasks.length === 0 && (
                        <p className="form-error">
                          当前 Task 没有上级 Task；请先在“修改任务”中建立关系。
                        </p>
                      )}
                      <legend>2. 选择上级 Task 输出 Sample</legend>
                      {eligibleInputSamples.map((sample) => (
                        <label key={sample.id}>
                          <input
                            type="checkbox"
                            checked={inputSampleIds.includes(sample.id)}
                            onChange={(event) =>
                              setInputSampleIds((current) =>
                                event.target.checked
                                  ? [...current, sample.id]
                                  : current.filter((id) => id !== sample.id),
                              )
                            }
                          />
                          <span>
                            <b>{sample.code}</b>
                            <small>
                              {sample.displayName ||
                                sampleTypeLabel(sample.type)}
                              {sample.metadata?.treatment_factor
                                ? ` · ${String(sample.metadata.treatment_factor)}`
                                : ""}
                              {sample.metadata?.treatment_duration
                                ? ` · ${String(sample.metadata.treatment_duration)}`
                                : ""}
                            </small>
                          </span>
                        </label>
                      ))}
                      {sourceTaskIds.length > 0 &&
                        eligibleInputSamples.length === 0 && (
                          <p className="form-error">
                            所选上级 Task 尚无符合此 Protocol 类型要求的输出
                            Sample。
                          </p>
                        )}
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
                    {record ? protocol?.name || "已有 Record" : "尚无 Record"}
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

function ProtocolsPage({ protocols }: { protocols: Store["protocols"] }) {
  return (
    <section className="page">
      <header>
        <div>
          <p className="eyebrow">PROTOCOL LIBRARY</p>
          <h1>实验 Protocol</h1>
          <p className="muted">结构化模板会在创建记录时保存版本快照。</p>
        </div>
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
            <span>当前版本 v{protocol.version}</span>
            <div>
              {protocol.blocks.map((block) => (
                <em key={block}>{block}</em>
              ))}
            </div>
            <footer>
              <button>查看版本 →</button>
            </footer>
          </article>
        ))}
      </div>
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
  const recordProtocol = store.protocols.find(
    (protocol) => protocol.id === record?.protocolId,
  );
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
    manifest: Awaited<ReturnType<typeof createExportManifest>>;
  }>();
  const [exportError, setExportError] = useState("");
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
      setExportPreview({ records: selectedRecords, manifest });
      setExportError("");
    } catch (reason) {
      setExportError(reason instanceof Error ? reason.message : String(reason));
    }
  };
  const printExport = async () => {
    if (!exportPreview) return;
    try {
      await markExportPrintRequested(exportPreview.manifest.id);
      await Promise.resolve(window.print());
    } catch (reason) {
      setExportError(reason instanceof Error ? reason.message : String(reason));
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
                          {protocol?.name}
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
            >
              返回选择
            </button>
            <span>
              {exportPreview.manifest.recordCount} 条 · 校验值{" "}
              {exportPreview.manifest.contentSha256.slice(0, 12)}…
            </span>
            <button className="primary" onClick={() => void printExport()}>
              打印 / 保存 PDF
            </button>
          </div>
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
                        {protocol?.name} · v{item.protocolVersion || "snapshot"}
                      </dd>
                    </div>
                    <div>
                      <dt>Record ID</dt>
                      <dd>{item.id}</dd>
                    </div>
                  </dl>
                  <section>
                    <h4>实验正文</h4>
                    <p className="export-body">
                      {item.renderedContent || item.notes || "暂无正文。"}
                    </p>
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
        </div>
      )}
      {record && (
        <div className="overlay record-overlay">
          <section className="record-panel">
            <header className="record-header">
              <button className="back" onClick={closeRecord}>
                ←
              </button>
              <div>
                <h1>{record.title}</h1>
                <p>本地实验记录 · 更新于 {record.updated}</p>
              </div>
              <button className="secondary" onClick={closeRecord}>
                完成
              </button>
            </header>
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
                  </div>
                  <p style={{ whiteSpace: "pre-wrap" }}>
                    {record.renderedContent || record.notes || "暂无正文。"}
                  </p>
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
                {recordProtocol?.terminalAssay && (
                  <TerminalAssayWorkspace
                    record={record}
                    samples={store.samples}
                    definition={recordProtocol.terminalAssay}
                    changed={changed}
                  />
                )}
              </article>
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
  const [title, setTitle] = useState(task.title),
    [experimentId, setExperimentId] = useState(task.experimentId),
    [newExperiment, setNewExperiment] = useState(false),
    [newExperimentName, setNewExperimentName] = useState(""),
    [parentTaskIds, setParentTaskIds] = useState(task.parentTaskIds || []),
    [start, setStart] = useState(task.start.slice(0, 16)),
    [end, setEnd] = useState(task.end.slice(0, 16)),
    [error, setError] = useState("");
  const editing = Boolean(task.title);
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
      <form className="modal task-form" onSubmit={save}>
        <button className="close" type="button" onClick={cancel}>
          ×
        </button>
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
        {!newExperiment && experimentId && (
          <fieldset className="task-dependencies">
            <legend>上级 Task（可多选）</legend>
            <p>当前任务将依赖所选任务；系统会阻止跨 Experiment 和循环关系。</p>
            {tasks
              .filter(
                (candidate) =>
                  candidate.experimentId === experimentId &&
                  candidate.id !== task.id,
              )
              .map((candidate) => (
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
            {tasks.filter(
              (candidate) =>
                candidate.experimentId === experimentId &&
                candidate.id !== task.id,
            ).length === 0 && <p>当前 Experiment 暂无可选的上级 Task。</p>}
          </fieldset>
        )}
        <div className="time-grid">
          <label>
            开始时间
            <input
              type="datetime-local"
              step="3600"
              value={start}
              onChange={(e) => setStart(e.target.value)}
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
        {error && <p className="form-error">{error}</p>}
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
