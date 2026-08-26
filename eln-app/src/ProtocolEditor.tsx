import { useMemo, useState } from "react";
import type { Protocol, SampleTypeDefinition } from "./domain";
import {
  saveProtocolTemplateVersion,
  saveUserProtocol,
  uid,
  type UserProtocolDraft,
} from "./repository";
import "./protocol-editor.css";

type OutputBehavior = UserProtocolDraft["outputBehavior"];

const canonicalType = (value: string) =>
  value
    .trim()
    .toUpperCase()
    .replace(/[^A-Z0-9_]/g, "_");

const outputLabels: Record<OutputBehavior, string> = {
  same_sample: "原 Sample 继续",
  derived_one: "产生新的 Sample",
  derived_multiple: "产生多个 Sample",
  measurement_only: "仅检测，不产生 Sample",
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
  const [name, setName] = useState(initialName);
  const [description, setDescription] = useState("");
  const [inputType, setInputType] = useState("RNA");
  const [outputBehavior, setOutputBehavior] =
    useState<OutputBehavior>("derived_one");
  const [outputType, setOutputType] = useState("CDNA");
  const [consumptionPolicy, setConsumptionPolicy] =
    useState<UserProtocolDraft["consumptionPolicy"]>("consume");
  const [template, setTemplate] = useState(
    "日期：{{date}}\n输入 Sample：{{input_sample_summary}}\n\nProcedure:\n1. \n2. \n3. \n\n输出 Sample：{{output_sample_summary}}",
  );
  const [error, setError] = useState("");
  const [saving, setSaving] = useState(false);
  const normalizedInput = canonicalType(inputType);
  const normalizedOutput = canonicalType(outputType);
  const selectedInput = sampleTypes.find(
    (item) => item.canonicalType === normalizedInput,
  );
  const selectedOutput = sampleTypes.find(
    (item) => item.canonicalType === normalizedOutput,
  );
  const inputPreview = `${selectedInput?.displayName || normalizedInput || "INPUT"}-001`;
  const outputPreview =
    outputBehavior === "measurement_only"
      ? "Measurement only"
      : outputBehavior === "same_sample"
        ? inputPreview
        : `${selectedOutput?.displayName || normalizedOutput || "OUTPUT"}-001${outputBehavior === "derived_multiple" ? " …" : ""}`;

  const validateStep = () => {
    if (step === 1 && (!name.trim() || !description.trim()))
      return "请填写名称和描述。";
    if (step === 2 && !normalizedInput) return "请选择或新建输入 Sample 类型。";
    if (
      step === 2 &&
      ["derived_one", "derived_multiple"].includes(outputBehavior) &&
      !normalizedOutput
    )
      return "请选择或新建输出 Sample 类型。";
    if (
      step === 2 &&
      outputBehavior === "same_sample" &&
      consumptionPolicy === "consume"
    )
      return "原 Sample 继续时不能同时将输入标记为已消耗。";
    if (step === 3 && !template.trim()) return "Record 实验正文不能为空。";
    return "";
  };

  const next = () => {
    const message = validateStep();
    if (message) return setError(message);
    setError("");
    setStep((current) => Math.min(3, current + 1));
  };

  const submit = async () => {
    const message = validateStep();
    if (message) return setError(message);
    setSaving(true);
    setError("");
    try {
      await saveUserProtocol({
        id: uid("protocol"),
        name: name.trim(),
        description: description.trim(),
        category: "自定义",
        accent: "#6957e8",
        inputType: normalizedInput,
        inputTypeDisplayName: selectedInput?.displayName || inputType.trim(),
        outputBehavior,
        outputType:
          outputBehavior === "derived_one" ||
          outputBehavior === "derived_multiple"
            ? normalizedOutput
            : undefined,
        outputTypeDisplayName: selectedOutput?.displayName || outputType.trim(),
        consumptionPolicy,
        template,
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
          {["基本信息", "Sample Flow", "Record Template"].map(
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
            <label>
              Protocol 名称
              <input
                value={name}
                onChange={(event) => setName(event.target.value)}
                placeholder="Reverse Transcription"
              />
            </label>
            <label>
              描述
              <textarea
                value={description}
                onChange={(event) => setDescription(event.target.value)}
                placeholder="说明这个 Protocol 用于什么实验过程"
                rows={5}
              />
            </label>
          </div>
        )}

        {step === 2 && (
          <div className="protocol-editor-body flow-layout">
            <div className="flow-form">
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
                {!selectedInput && inputType.trim() && (
                  <small>将新建类型：{normalizedInput}</small>
                )}
              </label>
              <fieldset>
                <legend>完成以后</legend>
                {(Object.keys(outputLabels) as OutputBehavior[]).map(
                  (behavior) => (
                    <label className="radio-row" key={behavior}>
                      <input
                        type="radio"
                        checked={outputBehavior === behavior}
                        onChange={() => {
                          setOutputBehavior(behavior);
                          if (behavior === "same_sample")
                            setConsumptionPolicy("retain");
                        }}
                      />
                      {outputLabels[behavior]}
                    </label>
                  ),
                )}
              </fieldset>
              {(outputBehavior === "derived_one" ||
                outputBehavior === "derived_multiple") && (
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
                  {!selectedOutput && outputType.trim() && (
                    <small>将新建类型：{normalizedOutput}</small>
                  )}
                </label>
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
            <aside className="flow-preview">
              <p>Sample Flow Preview</p>
              <strong>{inputPreview}</strong>
              <i>│</i>
              <span>│ {name.trim() || "Protocol"}</span>
              <i>↓</i>
              <strong>{outputPreview}</strong>
              <small>派生 Sample 默认继承父 Sample metadata。</small>
            </aside>
          </div>
        )}

        {step === 3 && (
          <div className="protocol-editor-body template-layout">
            <label>
              实验正文
              <textarea
                value={template}
                onChange={(event) => setTemplate(event.target.value)}
                rows={15}
              />
              <small>
                可用：{"{{date}}"}、{"{{input_sample_summary}}"}、
                {"{{output_sample_summary}}"}
              </small>
            </label>
            <aside className="record-template-preview">
              <p>Record Preview</p>
              <pre>
                {template
                  .replaceAll("{{date}}", new Date().toISOString().slice(0, 10))
                  .replaceAll("{{input_sample_summary}}", inputPreview)
                  .replaceAll(
                    "{{output_sample_summary}}",
                    outputBehavior === "measurement_only" ? "" : outputPreview,
                  )}
              </pre>
            </aside>
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
    () => protocol.templateVariants || {},
    [protocol],
  );
  const [template, setTemplate] = useState(protocol.template || "");
  const [variants, setVariants] =
    useState<Record<string, string>>(initialVariants);
  const [error, setError] = useState("");
  const [saving, setSaving] = useState(false);
  const submit = async () => {
    setSaving(true);
    setError("");
    try {
      await saveProtocolTemplateVersion({
        protocolId: protocol.id,
        template: protocol.templateVariants ? undefined : template,
        templateVariants: protocol.templateVariants ? variants : undefined,
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
            <p className="eyebrow">RECORD TEMPLATE</p>
            <h2>{protocol.name}</h2>
            <p className="muted">
              保存后生成 v{protocol.version + 1}；已有 Record 不会改变。
            </p>
          </div>
          <button className="back" onClick={close}>
            ×
          </button>
        </header>
        <div className="protocol-editor-body single-column">
          {protocol.templateVariants ? (
            Object.entries(variants).map(([key, value]) => (
              <label key={key}>
                {key}
                <textarea
                  rows={12}
                  value={value}
                  onChange={(event) =>
                    setVariants((current) => ({
                      ...current,
                      [key]: event.target.value,
                    }))
                  }
                />
              </label>
            ))
          ) : (
            <label>
              实验正文
              <textarea
                rows={16}
                value={template}
                onChange={(event) => setTemplate(event.target.value)}
              />
            </label>
          )}
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
