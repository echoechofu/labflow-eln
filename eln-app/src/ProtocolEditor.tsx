import { useEffect, useMemo, useState } from "react";
import type { Protocol, ProtocolField, SampleTypeDefinition } from "./domain";
import { ProtocolComposer } from "./ProtocolComposer";
import { validProtocolFieldKey } from "./protocolFieldKey";
import {
  groupSampleTypes,
  selectableSampleTypes,
  validCanonicalSampleType,
} from "./sampleTypeCatalog";
import {
  loadStore,
  saveProtocolTemplateVersion,
  saveUserProtocol,
  uid,
  type UserProtocolDraft,
} from "./repository";
import "./protocol-editor.css";

type OutputBehavior = NonNullable<UserProtocolDraft["outputBehavior"]>;
const canonicalType = (value: string) =>
  value
    .trim()
    .toUpperCase()
    .replace(/[^A-Z0-9_]/g, "_");
const validSampleType = (value: string) =>
  validCanonicalSampleType(canonicalType(value));
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
  const [inputTypes, setInputTypes] = useState<string[]>(["RNA"]);
  const [allowAnyInputType, setAllowAnyInputType] = useState(false);
  const [customInputTypes, setCustomInputTypes] = useState<
    SampleTypeDefinition[]
  >([]);
  const [customInputCode, setCustomInputCode] = useState("");
  const [customInputName, setCustomInputName] = useState("");
  const [outputBehavior, setOutputBehavior] =
    useState<OutputBehavior>("one_to_one");
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
    if (!source && step === 2 && !allowAnyInputType && inputTypes.length === 0)
      return "请至少选择一种适用的输入类型，或选择“不限类型”。";
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
        const availableTypes = [...sampleTypes, ...customInputTypes];
        await saveUserProtocol({
          ...base,
          fields,
          inputTypes: allowAnyInputType
            ? []
            : inputTypes.map((value) => {
                const canonical = canonicalType(value);
                const selected = availableTypes.find(
                  (item) => item.canonicalType === canonical,
                );
                return {
                  canonicalType: canonical,
                  displayName: selected?.displayName || canonical,
                };
              }),
          allowAnyInputType,
          outputBehavior,
          consumptionPolicy:
            outputBehavior === "one_to_many"
              ? consumptionPolicy
              : outputBehavior === "same_sample"
                ? "retain"
                : "consume",
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
              <fieldset className="applicable-input-types">
                <legend>适用的输入类型</legend>
                <p className="form-hint">
                  创建 Record 时只显示这些类型的 Sample；同一条 Record
                  的多个输入仍必须属于同一种类型。
                </p>
                <label className="radio-row input-type-any">
                  <input
                    type="checkbox"
                    checked={allowAnyInputType}
                    onChange={(event) => {
                      setAllowAnyInputType(event.target.checked);
                      if (event.target.checked) setInputTypes([]);
                    }}
                  />
                  不限类型（仅用于冻存、转移等真正通用的操作）
                </label>
                {!allowAnyInputType &&
                  groupSampleTypes([
                    ...selectableSampleTypes(sampleTypes),
                    ...customInputTypes,
                  ]).map((group) => (
                    <section className="input-type-category" key={group.label}>
                      <h4>{group.label}</h4>
                      <div className="input-type-options">
                        {group.items.map((item) => (
                          <label key={item.canonicalType}>
                            <input
                              type="checkbox"
                              checked={inputTypes.includes(item.canonicalType)}
                              onChange={(event) =>
                                setInputTypes((current) =>
                                  event.target.checked
                                    ? [...current, item.canonicalType]
                                    : current.filter(
                                        (value) => value !== item.canonicalType,
                                      ),
                                )
                              }
                            />
                            <span>{item.displayName}</span>
                            <code>{item.canonicalType}</code>
                          </label>
                        ))}
                      </div>
                    </section>
                  ))}
                {!allowAnyInputType && (
                  <details className="custom-input-type">
                    <summary>＋ 添加新的通用适用类型</summary>
                    <p className="form-hint">
                      仅当现有通用材料类别都不适用时才新增。动物取材 Protocol 的适用输入类型选择 ANIMAL；创建 Record 时输出类型选择 TISSUE，并在自定义名称中填写取材部位。代码须以英文字母开头，只能包含大写字母、数字和下划线。
                    </p>
                    <div>
                      <label>
                        显示名称
                        <input
                          value={customInputName}
                          placeholder="填写通用类型名称"
                          onChange={(event) => setCustomInputName(event.target.value)}
                        />
                      </label>
                      <label>
                        类型代码
                        <input
                          value={customInputCode}
                          placeholder="填写类型代码"
                          onChange={(event) =>
                            setCustomInputCode(canonicalType(event.target.value))
                          }
                        />
                      </label>
                      <button
                        type="button"
                        className="secondary"
                        onClick={() => {
                          const code = canonicalType(customInputCode);
                          if (!customInputName.trim() || !validSampleType(code)) {
                            setError("请填写显示名称，并使用有效的类型代码。");
                            return;
                          }
                          if (
                            [...sampleTypes, ...customInputTypes].some(
                              (item) => item.canonicalType === code,
                            )
                          ) {
                            setError("这个类型代码已经存在，请直接勾选对应类型。");
                            return;
                          }
                          setCustomInputTypes((current) => [
                            ...current,
                            {
                              canonicalType: code,
                              displayName: customInputName.trim(),
                              origin: "user",
                            },
                          ]);
                          setInputTypes((current) => [...current, code]);
                          setCustomInputCode("");
                          setCustomInputName("");
                          setError("");
                        }}
                      >
                        添加并选中
                      </button>
                    </div>
                  </details>
                )}
              </fieldset>
              <fieldset>
                <legend>完成以后</legend>
                {(
                  [
                    ["same_sample", "原 Sample 沿用"],
                    ["one_to_one", "1 → 1：原 Sample 消耗，产生一个新 Sample"],
                    ["one_to_many", "1 → 多：产生多个新 Sample"],
                    ["one_to_zero", "1 → 0：原 Sample 消耗，不产生 Sample"],
                  ] as [OutputBehavior, string][]
                ).map(([value, label]) => (
                  <label className="radio-row" key={value}>
                    <input
                      type="radio"
                      checked={outputBehavior === value}
                      onChange={() => {
                        setOutputBehavior(value);
                        if (value === "same_sample") setConsumptionPolicy("retain");
                        if (["one_to_one", "one_to_zero"].includes(value))
                          setConsumptionPolicy("consume");
                      }}
                    />
                    {label}
                  </label>
                ))}
              </fieldset>
              {outputBehavior === "one_to_many" && <fieldset>
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
              </fieldset>}
              <p className="form-hint">
                输出 Sample 的类型、名称和处理信息在创建 Record 时逐行填写。
              </p>
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
            />
            {!source && (
              <div className="system-field-note">
                <span>
                  系统按 Sample Flow 处理输入身份和消耗状态；新 Sample
                  的类型与具体信息会在创建 Record 时填写。这里添加的字段用于记录本次实验过程。
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
