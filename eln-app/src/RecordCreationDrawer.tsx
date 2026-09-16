import { useState } from "react";
import type { Experiment, Protocol, Task } from "./domain";
import {
  dayLabel,
  formatTime,
  normalizeSampleType,
  sampleTypeLabel,
  statusLabel,
} from "./domain";
import {
  startTaskRecord,
  uid,
  updateTaskStatus,
  type ExternalSampleDraft,
  type Store,
} from "./repository";
import { ProtocolCreationWizard } from "./ProtocolEditor";
import {
  eligibleRecordInputSamples,
  groupSamplesBySource,
  sampleSourceInfo,
  type SampleSourceKind,
} from "./taskInputs";
import { searchProtocols } from "./protocolSearch";
import { ProtocolFields } from "./ProtocolFields";
import type {
  PlateTreatmentGroup,
  SampleConditionGroup,
} from "./ProtocolLayoutEditors";

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

export function TaskDrawer({
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
  const [externalPlateFormat, setExternalPlateFormat] = useState("");
  const [externalSampleCount, setExternalSampleCount] = useState(1);
  const [externalSamples, setExternalSamples] = useState<
    { displayName: string; conditions: string }[]
  >([{ displayName: "", conditions: "" }]);
  const [plateGroups, setPlateGroups] = useState<PlateTreatmentGroup[]>([
    { factor: "", duration: "", wellCount: 1 },
  ]);
  const [conditionGroups, setConditionGroups] = useState<
    SampleConditionGroup[]
  >([{ condition: "", dose: "", duration: "", method: "", sampleCount: 1 }]);
  const protocolResults = searchProtocols(protocols, protocolQuery);
  const closeProtocolPicker = () => {
    setChoosingProtocol(false);
    setCreatingProtocol(false);
    setProtocol(undefined);
    setProtocolQuery("");
  };
  const usesExperimentSampleInput = [
    "parent_task_outputs",
    "experiment_samples",
  ].includes(protocol?.execution?.inputSource || "");
  const selectedInput = samples.find(
    (sample) => sample.id === values.input_sample,
  );
  const selectedInputType =
    (selectedInput && normalizeSampleType(selectedInput.type)) ||
    ({ 孔板: "PLATE", 培养皿: "DISH", 孔: "WELL" }[values.new_object_type] as
      string | undefined);
  const selectedExperimentInputs = samples.filter((sample) =>
    inputSampleIds.includes(sample.id),
  );
  const requiresUniformInputType =
    protocol?.execution?.inputTypePolicy === "uniform";
  const selectedCanonicalInputType = selectedExperimentInputs[0]
    ? normalizeSampleType(selectedExperimentInputs[0].type)
    : undefined;
  const activeInputTypes = usesExperimentSampleInput
    ? inputMode === "existing"
      ? selectedExperimentInputs.map((sample) =>
          normalizeSampleType(sample.type),
        )
      : externalSampleType
        ? [normalizeSampleType(externalSampleType)]
        : []
    : selectedInputType
      ? [selectedInputType]
      : [];
  const selectedPlateCapacities = selectedExperimentInputs
    .filter((sample) => normalizeSampleType(sample.type) === "PLATE")
    .map(
      (sample) =>
        plateCapacity(sample.metadata?.plate_capacity) ||
        plateCapacity(sample.metadata?.plate_format) ||
        plateCapacity(sample.metadata?.container_name),
    )
    .filter((capacity) => capacity > 0);
  const selectedPlateCapacity = usesExperimentSampleInput
    ? inputMode === "external"
      ? plateCapacity(externalPlateFormat)
      : selectedPlateCapacities.length
        ? Math.min(...selectedPlateCapacities)
        : 0
    : plateCapacity(selectedInput?.metadata?.plate_capacity) ||
      plateCapacity(selectedInput?.metadata?.plate_format) ||
      plateCapacity(selectedInput?.metadata?.container_name) ||
      plateCapacity(values.new_plate_format);
  const usesConditionAllocation =
    protocol?.execution?.outputMode === "per_input_conditions";
  const mapsConditionsToPlate =
    protocol?.execution?.conditionAllocation?.plateMapping === true;
  const conditionPlateCapacity = mapsConditionsToPlate
    ? plateCapacity(values.plate_format)
    : 0;
  const eligibleInputSamples = usesExperimentSampleInput
    ? eligibleRecordInputSamples(
        samples,
        task.experimentId,
        protocol?.execution?.inputTypes ?? [],
      )
    : [];
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
    const sample = samples.find((item) => item.id === sampleId);
    if (
      selected &&
      requiresUniformInputType &&
      selectedCanonicalInputType &&
      sample &&
      normalizeSampleType(sample.type) !== selectedCanonicalInputType
    ) {
      setError(
        `同一条 Record 的输入 Sample 必须属于同一种类型；请先取消已选的 ${sampleTypeLabel(selectedCanonicalInputType)}。`,
      );
      return;
    }
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
    const incompatibleType =
      requiresUniformInputType &&
      Boolean(selectedCanonicalInputType) &&
      normalizeSampleType(sample.type) !== selectedCanonicalInputType &&
      !inputSampleIds.includes(sample.id);
    const source = sampleSourceInfo(sample, task, tasks, records);
    const sourceText = source.sourceTask
      ? `来源：${source.sourceTask.title} · ${dayLabel(source.sourceTask.start)} ${formatTime(source.sourceTask.start)}`
      : source.kind === "external"
        ? "外部登记 · 无来源 Task"
        : "来源 Task 不可用";
    return (
      <label
        className={`sample-source-option${incompatibleType ? " incompatible" : ""}`}
        key={sample.id}
      >
        <input
          type="checkbox"
          checked={inputSampleIds.includes(sample.id)}
          disabled={incompatibleType}
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
            {incompatibleType
              ? ` · 已选择 ${sampleTypeLabel(selectedCanonicalInputType || "")}，不可混选`
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
    setExternalPlateFormat("");
    setExternalSampleCount(1);
    setExternalSamples([{ displayName: "", conditions: "" }]);
    setConditionGroups([
      { condition: "", dose: "", duration: "", method: "", sampleCount: 1 },
    ]);
    setPlateGroups([{ factor: "", duration: "", wellCount: 1 }]);
    const inputTypes = (item.execution?.inputTypes || []).map(
      normalizeSampleType,
    );
    const candidates = eligibleRecordInputSamples(
      samples,
      task.experimentId,
      inputTypes,
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
      usesExperimentSampleInput &&
      inputMode === "external" &&
      normalizeSampleType(externalSampleType) === "PLATE" &&
      !plateCapacity(externalPlateFormat)
    )
      return setError("请填写迁入孔板的规格。");
    if (
      protocol.fields?.some(
        (field) =>
          field.required &&
          field.kind !== "condition_groups" &&
          !values[field.key]?.trim(),
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
      activeInputTypes.includes("PLATE")
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
    if (
      protocol.execution?.eventType === "treatment" &&
      activeInputTypes.some((type) =>
        ["CELL", "DISH", "WELL"].includes(type),
      ) &&
      !values.treatment_type?.trim()
    )
      return setError("请填写 Cell / 培养皿 / 孔的刺激类型。");
    if (usesConditionAllocation) {
      if (conditionGroups.length === 0)
        return setError("请增加至少一个实验条件组。");
      const invalidGroup = conditionGroups.findIndex(
        (group) =>
          !group.condition.trim() ||
          !Number.isInteger(group.sampleCount) ||
          group.sampleCount < 1,
      );
      if (invalidGroup >= 0)
        return setError(
          `第 ${invalidGroup + 1} 组需要填写实验条件和有效的 Sample 数量。`,
        );
      const total = conditionGroups.reduce(
        (sum, group) => sum + group.sampleCount,
        0,
      );
      if (mapsConditionsToPlate && !conditionPlateCapacity)
        return setError("请选择孔板规格。");
      if (mapsConditionsToPlate && total > conditionPlateCapacity)
        return setError(
          `每个输入将分配 ${total} 个位置，超过 ${conditionPlateCapacity} 孔板容量。`,
        );
      if (!mapsConditionsToPlate && total > 384)
        return setError("每个输入最多产生 384 个按条件分配的 Sample。");
    }
    try {
      let submittedValues = values;
      if (
        protocol.execution?.eventType === "treatment" &&
        activeInputTypes.includes("PLATE")
      ) {
        submittedValues = {
          ...submittedValues,
          treatment_groups: JSON.stringify(plateGroups),
        };
      }
      if (usesConditionAllocation) {
        submittedValues = {
          ...submittedValues,
          condition_groups: JSON.stringify(conditionGroups),
        };
      }
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
              metadata: {
                ...(sample.conditions.trim()
                  ? { existing_conditions: sample.conditions.trim() }
                  : {}),
                ...(normalizeSampleType(externalSampleType) === "PLATE"
                  ? {
                      plate_format: externalPlateFormat,
                      plate_capacity: plateCapacity(externalPlateFormat),
                    }
                  : {}),
              },
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
                                {normalizeSampleType(externalSampleType) ===
                                  "PLATE" && (
                                  <label className="task-form">
                                    孔板规格
                                    <select
                                      value={externalPlateFormat}
                                      onChange={(event) =>
                                        setExternalPlateFormat(
                                          event.target.value,
                                        )
                                      }
                                    >
                                      <option value="">请选择</option>
                                      {[
                                        "6孔板",
                                        "12孔板",
                                        "24孔板",
                                        "48孔板",
                                        "96孔板",
                                        "384孔板",
                                      ].map((format) => (
                                        <option key={format}>{format}</option>
                                      ))}
                                    </select>
                                  </label>
                                )}
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
                  {protocol.execution?.outputRules?.length ? (
                    <div className="record-output-preview">
                      <b>本次 Record 的多类型输出</b>
                      <span>
                        每个输入 Sample 将产生：
                        {protocol.execution.outputRules
                          .map(
                            (rule) =>
                              `${sampleTypeLabel(rule.sampleType)} × ${rule.count}`,
                          )
                          .join("、")}
                      </span>
                      {inputSampleIds.length > 0 && (
                        <small>
                          当前选择 {inputSampleIds.length} 个输入，预计产生{" "}
                          {inputSampleIds.length *
                            protocol.execution.outputRules.reduce(
                              (total, rule) => total + rule.count,
                              0,
                            )}{" "}
                          个输出 Sample。
                        </small>
                      )}
                    </div>
                  ) : null}
                  {protocol.fields?.map((field) => (
                    <ProtocolFields
                      key={field.key}
                      field={field}
                      protocol={protocol}
                      samples={samples}
                      taskExperimentId={task.experimentId}
                      values={values}
                      inputTypes={activeInputTypes}
                      plateCapacity={selectedPlateCapacity}
                      conditionPlateCapacity={conditionPlateCapacity}
                      plateMapping={mapsConditionsToPlate}
                      plateGroups={plateGroups}
                      conditionGroups={conditionGroups}
                      setValue={(key, value) =>
                        setValues((current) => ({ ...current, [key]: value }))
                      }
                      setPlateGroups={(groups) => {
                        setPlateGroups(groups);
                        setError("");
                      }}
                      setConditionGroups={(groups) => {
                        setConditionGroups(groups);
                        setError("");
                      }}
                    />
                  ))}
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
