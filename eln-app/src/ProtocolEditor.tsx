import { useEffect, useMemo, useState } from "react";
import type { Protocol, ProtocolField, SampleTypeDefinition } from "./domain";
import { ProtocolComposer } from "./ProtocolComposer";
import { validProtocolFieldKey } from "./protocolFieldKey";
import {
  loadStore,
  saveProtocolTemplateVersion,
  saveUserProtocol,
  uid,
  type UserProtocolDraft,
} from "./repository";
import "./protocol-editor.css";

type OutputBehavior = NonNullable<UserProtocolDraft["outputBehavior"]>;
type MultipleSampleMode = NonNullable<UserProtocolDraft["multipleSampleMode"]>;
type OutputRuleDraft = NonNullable<UserProtocolDraft["outputRules"]>[number];
const canonicalType = (value: string) =>
  value
    .trim()
    .toUpperCase()
    .replace(/[^A-Z0-9_]/g, "_");
const validSampleType = (value: string) =>
  /^[A-Z][A-Z0-9_]{0,31}$/.test(canonicalType(value));
const simpleTemplate =
  "日期：{{date}}\n输入 Sample：{{input_sample_summary}}\n\nProcedure:\n1. \n2. \n3. \n\n输出 Sample：{{output_sample_summary}}";
const cloneFields = (fields?: ProtocolField[]) =>
  fields?.map((field) => ({
    ...field,
    options: field.options ? [...field.options] : undefined,
    visibleWhen: field.visibleWhen ? { ...field.visibleWhen } : undefined,
  })) || [];
const structuralKeys = new Set([
  "output_count",
  "plate_format",
  "condition_groups",
]);
const lockedProtocolFields = (protocol: Protocol) => {
  const fields = protocol.fields || [];
  if (protocol.origin === "builtin") return fields.map((field) => field.key);
  const locked = new Set(protocol.protectedFieldKeys || []);
  fields.forEach((field) => {
    if (
      structuralKeys.has(field.key) ||
      field.key === protocol.templateSelector
    )
      locked.add(field.key);
    if (field.visibleWhen?.key) locked.add(field.visibleWhen.key);
  });
  return [...locked];
};

export function ProtocolCreationWizard({
  sampleTypes,
  close,
  saved,
  initialName = "",
}: {
  sampleTypes: SampleTypeDefinition[];
  close: () => void;
  saved: () => void;
  initialName?: string;
}) {
  const [step, setStep] = useState(1);
  const [protocols, setProtocols] = useState<Protocol[]>([]);
  const [sourceId, setSourceId] = useState("");
  const [name, setName] = useState(initialName);
  const [description, setDescription] = useState("");
  const [inputType, setInputType] = useState("RNA");
  const [outputBehavior, setOutputBehavior] =
    useState<OutputBehavior>("derived_one");
  const [outputType, setOutputType] = useState("CDNA");
  const [outputRules, setOutputRules] = useState<OutputRuleDraft[]>([
    { outputType: "SUP", outputTypeDisplayName: "上清", count: 1 },
    { outputType: "RNA", outputTypeDisplayName: "RNA", count: 1 },
    { outputType: "PROTEIN", outputTypeDisplayName: "蛋白", count: 1 },
  ]);
  const [multipleSampleMode, setMultipleSampleMode] =
    useState<MultipleSampleMode>("identical");
  const [conditionContainer, setConditionContainer] = useState<
    "independent" | "plate" | "dish"
  >("independent");
  const [consumptionPolicy, setConsumptionPolicy] = useState<
    "retain" | "consume"
  >("consume");
  const [template, setTemplate] = useState(simpleTemplate);
  const [variants, setVariants] = useState<Record<string, string>>({});
  const [fields, setFields] = useState<ProtocolField[]>([]);
  const [lockedKeys, setLockedKeys] = useState<string[]>([]);
  const [error, setError] = useState("");
  const [saving, setSaving] = useState(false);
  const source = protocols.find((item) => item.id === sourceId);

  useEffect(() => {
    loadStore()
      .then((store) => setProtocols(store.protocols))
      .catch((cause) => setError(String(cause)));
  }, []);
  const chooseSource = (id: string) => {
    setSourceId(id);
    const selected = protocols.find((item) => item.id === id);
    if (!selected) {
      setFields([]);
      setLockedKeys([]);
      setVariants({});
      setTemplate(simpleTemplate);
      return;
    }
    const inherited = cloneFields(selected.fields);
    setFields(inherited);
    setLockedKeys(inherited.map((field) => field.key));
    setTemplate(selected.template || "");
    setVariants({ ...(selected.templateVariants || {}) });
    if (!name.trim() || (source && name === `${source.name}（自定义）`))
      setName(`${selected.name}（自定义）`);
    if (!description.trim()) setDescription(selected.description || "");
  };
  const validate = () => {
    if (step === 1 && (!name.trim() || !description.trim()))
      return "请填写名称和描述。";
    if (!source && step === 2 && !validSampleType(inputType))
      return "请选择或新建输入 Sample 类型。";
    if (
      !source &&
      step === 2 &&
      ["derived_one", "derived_multiple"].includes(outputBehavior) &&
      !validSampleType(outputType)
    )
      return "请选择或新建输出 Sample 类型。";
    if (!source && step === 2 && outputBehavior === "derived_multi_type") {
      const types = outputRules.map((rule) => canonicalType(rule.outputType));
      if (
        outputRules.length < 2 ||
        outputRules.length > 16 ||
        outputRules.some((rule) => !validSampleType(rule.outputType)) ||
        new Set(types).size !== types.length
      )
        return "请添加 2–16 种互不重复的输出 Sample 类型。";
      if (
        outputRules.some(
          (rule) =>
            !Number.isInteger(rule.count) || rule.count < 1 || rule.count > 96,
        ) ||
        outputRules.reduce((total, rule) => total + rule.count, 0) > 96
      )
        return "每种输出数量须为 1–96，且每个输入的输出总数不能超过 96。";
    }
    if (
      !source &&
      step === 2 &&
      outputBehavior === "same_sample" &&
      consumptionPolicy === "consume"
    )
      return "原 Sample 继续时不能同时将输入标记为已消耗。";
    if (
      step === 3 &&
      !template.trim() &&
      !Object.values(variants).some((value) => value.trim())
    )
      return "Record 实验正文不能为空。";
    const keys = fields.map((field) => field.key.trim());
    if (
      step === 3 &&
      (keys.some((key) => !validProtocolFieldKey(key)) ||
        new Set(keys).size !== keys.length ||
        fields.some((field) => !field.label.trim()))
    )
      return "字段 Key 最多 64 个字符，只能使用英文字母、数字和下划线，必须以字母或下划线开头且不能重复。";
    if (
      step === 3 &&
      fields.some(
        (field) =>
          field.kind === "select" &&
          (!field.options?.length ||
            field.options.some((option) => !option.trim())),
      )
    )
      return "选择字段至少需要一个选项，且不能包含空行。";
    return "";
  };
  const next = () => {
    const message = validate();
    if (message) return setError(message);
    setError("");
    setStep((current) => Math.min(3, current + 1));
  };
  const submit = async () => {
    const keys = fields.map((field) => field.key.trim());
    if (
      keys.some((key) => !validProtocolFieldKey(key)) ||
      new Set(keys).size !== keys.length ||
      fields.some((field) => !field.label.trim())
    ) {
      setError(
        "字段 Key 最多 64 个字符，只能使用英文字母、数字和下划线，必须以字母或下划线开头且不能重复。",
      );
      return;
    }
    const message = validate();
    if (message) return setError(message);
    setSaving(true);
    setError("");
    try {
      const base = {
        id: uid("protocol"),
        name: name.trim(),
        description: description.trim(),
        category: "自定义",
        accent: source?.accent || "#6957e8",
        createdAt: new Date().toISOString(),
      };
      if (source)
        await saveUserProtocol({
          ...base,
          sourceProtocolId: source.id,
          fields,
          template: Object.keys(variants).length ? undefined : template,
          templateVariants: Object.keys(variants).length ? variants : undefined,
        });
      else {
        const selectedInput = sampleTypes.find(
          (item) => item.canonicalType === canonicalType(inputType),
        );
        const selectedOutput = sampleTypes.find(
          (item) => item.canonicalType === canonicalType(outputType),
        );
        const normalizedOutputRules = outputRules.map((rule) => {
          const canonical = canonicalType(rule.outputType);
          const registered = sampleTypes.find(
            (item) => item.canonicalType === canonical,
          );
          return {
            outputType: canonical,
            outputTypeDisplayName:
              registered?.displayName ||
              rule.outputTypeDisplayName ||
              rule.outputType.trim(),
            count: rule.count,
          };
        });
        await saveUserProtocol({
          ...base,
          fields,
          inputType: canonicalType(inputType),
          inputTypeDisplayName: selectedInput?.displayName || inputType.trim(),
          outputBehavior,
          multipleSampleMode:
            outputBehavior === "derived_multiple"
              ? multipleSampleMode
              : undefined,
          plateMapping:
            outputBehavior === "derived_multiple" &&
            multipleSampleMode === "condition_groups"
              ? conditionContainer === "plate"
              : undefined,
          conditionContainer:
            outputBehavior === "derived_multiple" &&
            multipleSampleMode === "condition_groups"
              ? conditionContainer
              : undefined,
          outputType: ["derived_one", "derived_multiple"].includes(
            outputBehavior,
          )
            ? canonicalType(outputType)
            : undefined,
          outputTypeDisplayName:
            selectedOutput?.displayName || outputType.trim(),
          outputRules:
            outputBehavior === "derived_multi_type"
              ? normalizedOutputRules
              : undefined,
          consumptionPolicy,
          template,
        });
      }
      saved();
      close();
    } catch (cause) {
      setError(String(cause));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="overlay centered protocol-editor-overlay">
      <section className="protocol-editor">
        <header className="protocol-editor-head">
          <div>
            <p className="eyebrow">NEW PROTOCOL</p>
            <h2>新增 Protocol</h2>
          </div>
          <button className="back" onClick={close} aria-label="关闭">
            ×
          </button>
        </header>
        <ol className="protocol-steps">
          {["基本信息", source ? "继承能力" : "Sample Flow", "正文与字段"].map(
            (label, index) => (
              <li
                className={
                  step === index + 1 ? "active" : step > index + 1 ? "done" : ""
                }
                key={label}
              >
                <span>{index + 1}</span>
                {label}
              </li>
            ),
          )}
        </ol>
        {step === 1 && (
          <div className="protocol-editor-body single-column">
            <fieldset className="creation-mode">
              <legend>创建方式</legend>
              <label className={`flow-choice ${!sourceId ? "selected" : ""}`}>
                <input
                  type="radio"
                  checked={!sourceId}
                  onChange={() => chooseSource("")}
                />
                <span>
                  <b>从头创建</b>
                  <small>
                    配置基础 Sample Flow，并添加自己的 Record 字段。
                  </small>
                </span>
              </label>
              <label className={`flow-choice ${sourceId ? "selected" : ""}`}>
                <input
                  type="radio"
                  checked={Boolean(sourceId)}
                  onChange={() => chooseSource(protocols[0]?.id || "")}
                />
                <span>
                  <b>从已有 Protocol 创建</b>
                  <small>复制其当前版本的字段、正文和完整执行能力。</small>
                </span>
              </label>
              {sourceId && (
                <select
                  value={sourceId}
                  onChange={(event) => chooseSource(event.target.value)}
                >
                  {protocols.map((item) => (
                    <option key={item.id} value={item.id}>
                      {item.name} · v{item.version}
                    </option>
                  ))}
                </select>
              )}
            </fieldset>
            <label>
              Protocol 名称
              <input
                value={name}
                onChange={(event) => setName(event.target.value)}
              />
            </label>
            <label>
              描述
              <textarea
                rows={4}
                value={description}
                onChange={(event) => setDescription(event.target.value)}
              />
            </label>
          </div>
        )}
        {step === 2 &&
          (source ? (
            <div className="protocol-editor-body">
              <section className="inheritance-summary">
                <p className="eyebrow">INHERITED EXECUTION</p>
                <h3>
                  {source.name} · v{source.version}
                </h3>
                <p>
                  输入、输出、检测、条件分配及其他执行语义均来自该 Protocol
                  的当前版本。此处不将它简化成新的 Sample Flow 设置。
                </p>
                <dl>
                  <div>
                    <dt>输入</dt>
                    <dd>
                      {source.execution?.inputTypes?.join(" / ") ||
                        "由来源执行器决定"}
                    </dd>
                  </div>
                  <div>
                    <dt>输出</dt>
                    <dd>
                      {source.execution?.outputRules?.length
                        ? source.execution.outputRules
                            .map((rule) => `${rule.sampleType} × ${rule.count}`)
                            .join(" / ")
                        : source.execution?.outputType ||
                          source.execution?.outputMode ||
                          "由来源执行器决定"}
                    </dd>
                  </div>
                  <div>
                    <dt>字段</dt>
                    <dd>{source.fields?.length || 0} 个来源字段</dd>
                  </div>
                </dl>
              </section>
            </div>
          ) : (
            <div className="protocol-editor-body flow-form">
              <label>
                输入 Sample 类型
                <input
                  list="protocol-input-types"
                  value={inputType}
                  onChange={(event) => setInputType(event.target.value)}
                />
                <datalist id="protocol-input-types">
                  {sampleTypes.map((item) => (
                    <option value={item.displayName} key={item.canonicalType}>
                      {item.canonicalType}
                    </option>
                  ))}
                </datalist>
              </label>
              <fieldset>
                <legend>完成以后</legend>
                {(
                  [
                    ["same_sample", "原 Sample 继续"],
                    ["derived_one", "产生新的 Sample"],
                    ["derived_multiple", "产生多个 Sample"],
                    ["derived_multi_type", "产生多种类型的 Sample"],
                    ["measurement_only", "仅检测，不产生 Sample"],
                  ] as [OutputBehavior, string][]
                ).map(([value, label]) => (
                  <label className="radio-row" key={value}>
                    <input
                      type="radio"
                      checked={outputBehavior === value}
                      onChange={() => {
                        setOutputBehavior(value);
                        if (value === "same_sample")
                          setConsumptionPolicy("retain");
                      }}
                    />
                    {label}
                  </label>
                ))}
              </fieldset>
              {["derived_one", "derived_multiple"].includes(outputBehavior) && (
                <label>
                  输出 Sample 类型
                  <input
                    list="protocol-output-types"
                    value={outputType}
                    onChange={(event) => setOutputType(event.target.value)}
                  />
                  <datalist id="protocol-output-types">
                    {sampleTypes.map((item) => (
                      <option value={item.displayName} key={item.canonicalType}>
                        {item.canonicalType}
                      </option>
                    ))}
                  </datalist>
                </label>
              )}
              {outputBehavior === "derived_multiple" && (
                <fieldset>
                  <legend>多个 Sample 的关系</legend>
                  <label className="radio-row">
                    <input
                      type="radio"
                      checked={multipleSampleMode === "identical"}
                      onChange={() => setMultipleSampleMode("identical")}
                    />
                    相同条件，仅填写数量
                  </label>
                  <label className="radio-row">
                    <input
                      type="radio"
                      checked={multipleSampleMode === "condition_groups"}
                      onChange={() => setMultipleSampleMode("condition_groups")}
                    />
                    按实验条件分配
                  </label>
                  {multipleSampleMode === "condition_groups" && (
                    <small>
                      下一步在模块编排中选择独立培养皿等容器，或加入孔板位置映射。
                    </small>
                  )}
                </fieldset>
              )}
              {outputBehavior === "derived_multi_type" && (
                <fieldset className="multi-output-rules">
                  <legend>每个输入 Sample 的输出</legend>
                  <p>
                    每条规则分别设置 Sample 类型和数量；创建 Record
                    时会对每个输入执行全部规则。
                  </p>
                  <div className="multi-output-rule multi-output-rule-head">
                    <b>Sample 类型</b>
                    <b>数量</b>
                    <span />
                  </div>
                  {outputRules.map((rule, index) => (
                    <div className="multi-output-rule" key={index}>
                      <input
                        list="protocol-multi-output-types"
                        aria-label={`第 ${index + 1} 种输出 Sample 类型`}
                        value={rule.outputType}
                        onChange={(event) =>
                          setOutputRules((current) =>
                            current.map((item, itemIndex) =>
                              itemIndex === index
                                ? {
                                    ...item,
                                    outputType: event.target.value,
                                    outputTypeDisplayName: event.target.value,
                                  }
                                : item,
                            ),
                          )
                        }
                      />
                      <input
                        type="number"
                        min="1"
                        max="96"
                        aria-label={`第 ${index + 1} 种输出数量`}
                        value={rule.count}
                        onChange={(event) =>
                          setOutputRules((current) =>
                            current.map((item, itemIndex) =>
                              itemIndex === index
                                ? { ...item, count: Number(event.target.value) }
                                : item,
                            ),
                          )
                        }
                      />
                      <button
                        type="button"
                        disabled={outputRules.length <= 2}
                        onClick={() =>
                          setOutputRules((current) =>
                            current.filter(
                              (_, itemIndex) => itemIndex !== index,
                            ),
                          )
                        }
                      >
                        删除
                      </button>
                    </div>
                  ))}
                  <datalist id="protocol-multi-output-types">
                    {sampleTypes.map((item) => (
                      <option
                        value={item.canonicalType}
                        key={item.canonicalType}
                      >
                        {item.displayName}
                      </option>
                    ))}
                  </datalist>
                  <button
                    className="secondary add-output-rule"
                    type="button"
                    disabled={outputRules.length >= 16}
                    onClick={() =>
                      setOutputRules((current) => [
                        ...current,
                        {
                          outputType: "",
                          outputTypeDisplayName: "",
                          count: 1,
                        },
                      ])
                    }
                  >
                    ＋ 添加输出类型
                  </button>
                  <small>
                    类型代码使用 1–32 位英文字母、数字或下划线，并以字母开头。
                  </small>
                  <small>
                    当前每个输入将产生{" "}
                    {outputRules.reduce(
                      (total, rule) => total + (Number(rule.count) || 0),
                      0,
                    )}{" "}
                    个输出 Sample。
                  </small>
                </fieldset>
              )}
              <fieldset>
                <legend>输入 Sample</legend>
                <label className="radio-row">
                  <input
                    type="radio"
                    checked={consumptionPolicy === "retain"}
                    onChange={() => setConsumptionPolicy("retain")}
                  />
                  保留
                </label>
                <label className="radio-row">
                  <input
                    type="radio"
                    checked={consumptionPolicy === "consume"}
                    onChange={() => setConsumptionPolicy("consume")}
                  />
                  视为已转化/消耗
                </label>
              </fieldset>
            </div>
          ))}
        {step === 3 && (
          <div className="protocol-editor-body editor-content-grid">
            <ProtocolComposer
              template={template}
              variants={variants}
              fields={fields}
              onTemplate={setTemplate}
              onVariants={setVariants}
              onFields={setFields}
              lockedKeys={lockedKeys}
              templateSelector={source?.templateSelector}
              conditionMapping={
                !source &&
                outputBehavior === "derived_multiple" &&
                multipleSampleMode === "condition_groups"
                  ? {
                      mode: conditionContainer,
                      onChange: setConditionContainer,
                    }
                  : undefined
              }
            />
            {!source && (
              <div className="system-field-note">
                {outputBehavior === "derived_multi_type" && (
                  <b>
                    多类型输出：
                    {outputRules
                      .map(
                        (rule) =>
                          `${canonicalType(rule.outputType)} × ${rule.count}`,
                      )
                      .join("、")}
                    。规则会分别应用到每个输入 Sample。
                  </b>
                )}
                <span>
                  系统会根据 Sample Flow 处理输出；需要运行时填写数量或条件时，
                  会自动加入相应字段。这里添加的 Record 字段会与它们合并。
                </span>
              </div>
            )}
          </div>
        )}
        {error && <p className="form-error protocol-editor-error">{error}</p>}
        <footer className="protocol-editor-actions">
          <button
            className="secondary"
            onClick={
              step === 1
                ? close
                : () => {
                    setStep(step - 1);
                    setError("");
                  }
            }
          >
            {step === 1 ? "取消" : "上一步"}
          </button>
          <button
            className="primary"
            disabled={saving}
            onClick={step === 3 ? submit : next}
          >
            {saving ? "保存中…" : step === 3 ? "创建 Protocol v1" : "下一步"}
          </button>
        </footer>
      </section>
    </div>
  );
}

export function ProtocolTemplateEditor({
  protocol,
  close,
  saved,
}: {
  protocol: Protocol;
  close: () => void;
  saved: () => void;
}) {
  const initialVariants = useMemo(
    () => ({ ...(protocol.templateVariants || {}) }),
    [protocol],
  );
  const [protocols, setProtocols] = useState<Protocol[]>([]);
  const [sourceId, setSourceId] = useState("");
  const [template, setTemplate] = useState(protocol.template || "");
  const [variants, setVariants] =
    useState<Record<string, string>>(initialVariants);
  const [fields, setFields] = useState<ProtocolField[]>(
    cloneFields(protocol.fields),
  );
  const [lockedKeys, setLockedKeys] = useState<string[]>(
    lockedProtocolFields(protocol),
  );
  const [error, setError] = useState("");
  const [saving, setSaving] = useState(false);
  useEffect(() => {
    loadStore()
      .then((store) =>
        setProtocols(store.protocols.filter((item) => item.id !== protocol.id)),
      )
      .catch((cause) => setError(String(cause)));
  }, [protocol.id]);
  const chooseSource = (id: string) => {
    setSourceId(id);
    const selected = protocols.find((item) => item.id === id);
    if (!selected) {
      setTemplate(protocol.template || "");
      setVariants(initialVariants);
      setFields(cloneFields(protocol.fields));
      setLockedKeys(lockedProtocolFields(protocol));
      return;
    }
    const inherited = cloneFields(selected.fields);
    setFields(inherited);
    setLockedKeys(inherited.map((field) => field.key));
    setTemplate(selected.template || "");
    setVariants({ ...(selected.templateVariants || {}) });
  };
  const submit = async () => {
    const keys = fields.map((field) => field.key.trim());
    if (
      keys.some((key) => !validProtocolFieldKey(key)) ||
      new Set(keys).size !== keys.length ||
      fields.some((field) => !field.label.trim())
    ) {
      setError(
        "字段 Key 最多 64 个字符，只能使用英文字母、数字和下划线，必须以字母或下划线开头且不能重复。",
      );
      return;
    }
    if (
      fields.some(
        (field) =>
          field.kind === "select" &&
          (!field.options?.length ||
            field.options.some((option) => !option.trim())),
      )
    ) {
      setError("选择字段至少需要一个选项，且不能包含空行。");
      return;
    }
    setSaving(true);
    setError("");
    try {
      await saveProtocolTemplateVersion({
        protocolId: protocol.id,
        sourceProtocolId: sourceId || undefined,
        fields,
        template: Object.keys(variants).length ? undefined : template,
        templateVariants: Object.keys(variants).length ? variants : undefined,
        createdAt: new Date().toISOString(),
      });
      saved();
      close();
    } catch (cause) {
      setError(String(cause));
    } finally {
      setSaving(false);
    }
  };
  return (
    <div className="overlay centered protocol-editor-overlay">
      <section className="protocol-editor template-editor">
        <header className="protocol-editor-head">
          <div>
            <p className="eyebrow">PROTOCOL VERSION</p>
            <h2>{protocol.name}</h2>
            <p className="muted">
              保存后生成 v{protocol.version + 1}；已有 Record 保持原快照。
            </p>
          </div>
          <button className="back" onClick={close}>
            ×
          </button>
        </header>
        <div className="protocol-editor-body single-column">
          <label>
            版本能力来源
            <select
              value={sourceId}
              onChange={(event) => chooseSource(event.target.value)}
            >
              <option value="">保留当前 Protocol 执行能力</option>
              {protocols.map((item) => (
                <option key={item.id} value={item.id}>
                  切换为 {item.name} · v{item.version}
                </option>
              ))}
            </select>
            <small>
              {sourceId
                ? "新版本将复制所选来源的完整执行能力。"
                : "只更新正文和字段，不改变当前执行能力。"}
            </small>
          </label>
          <ProtocolComposer
            template={template}
            variants={variants}
            fields={fields}
            onTemplate={setTemplate}
            onVariants={setVariants}
            onFields={setFields}
            lockedKeys={lockedKeys}
            templateSelector={
              (protocols.find((item) => item.id === sourceId) || protocol)
                .templateSelector
            }
          />
        </div>
        {error && <p className="form-error protocol-editor-error">{error}</p>}
        <footer className="protocol-editor-actions">
          <button className="secondary" onClick={close}>
            取消
          </button>
          <button className="primary" disabled={saving} onClick={submit}>
            {saving ? "保存中…" : `保存为 v${protocol.version + 1}`}
          </button>
        </footer>
      </section>
    </div>
  );
}
