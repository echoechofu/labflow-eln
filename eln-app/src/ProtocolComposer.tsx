import { useState } from "react";
import type { ProtocolField } from "./domain";
import { ProtocolFieldEditor } from "./ProtocolFieldEditor";
import { isProtocolFieldVisible } from "./protocolFieldVisibility";
import { useComposerDrag } from "./useComposerDrag";

const kinds = [
  { kind: "text", label: "文字" },
  { kind: "number", label: "数字" },
  { kind: "select", label: "下拉选项" },
] as const;

export function ProtocolComposer({
  template,
  variants,
  fields,
  onTemplate,
  onVariants,
  onFields,
  lockedKeys,
  templateSelector,
  conditionMapping,
}: {
  template: string;
  variants: Record<string, string>;
  fields: ProtocolField[];
  onTemplate: (value: string) => void;
  onVariants: (value: Record<string, string>) => void;
  onFields: (value: ProtocolField[]) => void;
  lockedKeys: string[];
  templateSelector?: string;
  conditionMapping?: {
    mode: "independent" | "plate" | "dish";
    onChange: (value: "independent" | "plate" | "dish") => void;
  };
}) {
  const [selectedVariant, setSelectedVariant] = useState("");
  const [trial, setTrial] = useState<Record<string, string>>({});
  const [inputType, setInputType] = useState("");
  const variant = Object.hasOwn(variants, selectedVariant)
    ? selectedVariant
    : Object.keys(variants)[0];
  const body = variant === undefined ? template : variants[variant];
  const [draft, setDraft] = useState({
    body,
    variant,
    paragraphs: body.split("\n\n"),
  });
  const current =
    draft.body === body && draft.variant === variant
      ? draft
      : { body, variant, paragraphs: body.split("\n\n") };
  if (current !== draft) setDraft(current);
  const paragraphs = current.paragraphs;
  const write = (next: string[]) => {
    const nextBody = next.join("\n\n");
    setDraft({ body: nextBody, variant, paragraphs: next });
    if (variant === undefined) onTemplate(nextBody);
    else onVariants({ ...variants, [variant]: nextBody });
  };
  const values = Object.fromEntries(
    fields.map((field) => [
      field.key,
      trial[field.key] ?? field.defaultValue ?? "",
    ]),
  );
  const types = [
    ...new Set(fields.flatMap((field) => field.visibleForInputTypes || [])),
  ];
  const actualInputType = types.includes(inputType)
    ? inputType
    : types[0] || "";
  if (templateSelector && variant !== undefined)
    values[templateSelector] = variant;
  const add = (kind: string) => {
    if (kind === "plate") {
      conditionMapping?.onChange("plate");
      return;
    }
    if (kind === "body") {
      write([...paragraphs, "新的实验步骤"]);
      return;
    }
    if (kind !== "text" && kind !== "number" && kind !== "select") return;
    let n = 1;
    while (fields.some((field) => field.key === `field_${n}`)) n++;
    onFields([
      ...fields,
      {
        key: `field_${n}`,
        label: kinds.find((item) => item.kind === kind)!.label,
        kind,
        ...(kind === "select" ? { options: ["对照", "处理"] } : {}),
      },
    ]);
  };
  const move = (from: number, to: number) => {
    if (
      !Number.isInteger(from) ||
      from < 0 ||
      from >= paragraphs.length ||
      to < 0 ||
      to >= paragraphs.length
    )
      return;
    const next = [...paragraphs];
    const [item] = next.splice(from, 1);
    next.splice(to, 0, item);
    write(next);
  };
  const moduleDrag = useComposerDrag((kind, target) => {
    if (target.closest(".composer-canvas")) add(kind);
  });
  const paragraphDrag = useComposerDrag((from, target) => {
    const to = target
      .closest("[data-paragraph]")
      ?.getAttribute("data-paragraph");
    if (to != null) move(Number(from), Number(to));
  });
  const tokens = [
    { key: "date", label: "日期" },
    { key: "input_sample_summary", label: "输入样本" },
    { key: "output_sample_summary", label: "输出样本" },
    ...fields,
  ];
  const previewValues: Record<string, string> = {
    ...values,
    date: "示例日期",
    input_sample_summary: "示例输入样本",
    output_sample_summary: "示例输出样本",
  };
  const preview = body.replace(
    /\{\{([^{}]+)\}\}/g,
    (_, key: string) =>
      previewValues[key] ||
      `〔${tokens.find((item) => item.key === key)?.label || key}：未填写〕`,
  );
  return (
    <div className="protocol-composer">
      <aside className="composer-library">
        <h3>模块库</h3>
        <p>点击添加，或拖到中间配置区。正文段落与字段分别排序。</p>
        {[{ kind: "body", label: "正文段落" }, ...kinds].map((item) => (
          <button
            type="button"
            className="drag-handle"
            key={item.kind}
            {...moduleDrag(item.kind, () => add(item.kind))}
          >
            ⠿ ＋ {item.label}
          </button>
        ))}
        {conditionMapping && (
          <button
            type="button"
            className="drag-handle"
            {...moduleDrag("plate", () => add("plate"))}
          >
            ⠿ ＋ 孔板位置
          </button>
        )}
      </aside>
      <main
        className="composer-canvas"
        onDragOver={(event) => event.preventDefault()}
        onDrop={(event) => {
          const kind = event.dataTransfer.getData("application/labflow-module");
          if (kind) {
            event.preventDefault();
            add(kind);
          }
        }}
      >
        {conditionMapping && (
          <section className="composer-paragraph">
            <h3>按条件分配 · 容器与位置</h3>
            <p>
              每组的处理方式、浓度、时间和 Sample 数量在创建 Record 时填写。
            </p>
            <label>
              容器与位置分配
              <select
                value={conditionMapping.mode}
                onChange={(event) =>
                  conditionMapping.onChange(
                    event.target.value as "independent" | "plate" | "dish",
                  )
                }
              >
                <option value="independent">
                  独立容器（如培养皿），不分配孔位
                </option>
                <option value="plate">孔板：按分组分配孔位</option>
                <option value="dish">培养皿：逐皿填写处理条件和方法</option>
              </select>
            </label>
            <p>
              {conditionMapping.mode === "plate"
                ? "创建 Record 时选择孔板规格，系统按分组数量分配孔位并检查容量。"
                : conditionMapping.mode === "dish"
                  ? "创建 Record 时，每个条件组可填写处理条件、浓度、时间、方法和皿数；输出将编号为皿 01、皿 02……"
                  : "按条件生成多个 Sample，不添加容器编号。"}{" "}
              Sample 类型继续使用上一步的选择。
            </p>
          </section>
        )}
        <h3>正文编排</h3>
        <p>拖动段落调整实验步骤；点击字段名称可插入到段落末尾。</p>
        {variant !== undefined && (
          <label>
            正文版本
            <select
              value={variant}
              onChange={(event) => setSelectedVariant(event.target.value)}
            >
              {Object.keys(variants).map((key) => (
                <option key={key}>{key}</option>
              ))}
            </select>
          </label>
        )}
        {paragraphs.map((paragraph, index) => (
          <section
            className="composer-paragraph"
            key={index}
            data-paragraph={index}
          >
            <div className="module-actions">
              <button
                type="button"
                className="drag-handle"
                {...paragraphDrag(String(index))}
              >
                ⠿ 段落 {index + 1}
              </button>
              <button
                type="button"
                disabled={index === 0}
                onClick={() => move(index, index - 1)}
              >
                上移
              </button>
              <button
                type="button"
                disabled={index === paragraphs.length - 1}
                onClick={() => move(index, index + 1)}
              >
                下移
              </button>
              <button
                type="button"
                onClick={() => write(paragraphs.filter((_, i) => i !== index))}
              >
                删除段落
              </button>
            </div>
            <textarea
              aria-label={`段落 ${index + 1} 正文`}
              rows={Math.min(8, Math.max(3, paragraph.split("\n").length))}
              value={paragraph}
              onChange={(event) =>
                write(
                  paragraphs.map((part, i) =>
                    i === index ? event.target.value : part,
                  ),
                )
              }
            />
            <div className="composer-tokens">
              <span>插入：</span>
              {tokens.map((token) => (
                <button
                  type="button"
                  key={token.key}
                  onClick={() =>
                    write(
                      paragraphs.map((part, i) =>
                        i === index ? `${part}{{${token.key}}}` : part,
                      ),
                    )
                  }
                >
                  {token.label}
                </button>
              ))}
            </div>
          </section>
        ))}
        <ProtocolFieldEditor
          fields={fields}
          onChange={onFields}
          lockedKeys={lockedKeys}
        />
      </main>
      <aside className="composer-preview">
        {conditionMapping && (
          <p>
            条件分组：
            {conditionMapping.mode === "plate"
              ? "分配孔板位置"
              : conditionMapping.mode === "dish"
                ? "逐皿分配"
                : "独立容器，不分配位置"}
          </p>
        )}
        <h3>Record 试填预览</h3>
        <p>试填不会保存为实验数据。样本与布局请在创建 Record 时配置。</p>
        {types.length > 0 && (
          <label>
            模拟输入类型
            <select
              value={actualInputType}
              onChange={(event) => setInputType(event.target.value)}
            >
              {types.map((type) => (
                <option key={type}>{type}</option>
              ))}
            </select>
          </label>
        )}
        {fields
          .filter((field) =>
            isProtocolFieldVisible(field, values, [actualInputType]),
          )
          .map((field) => (
            <label key={field.key}>
              {field.label}
              {field.required ? " *" : ""}
              {field.unit ? `（${field.unit}）` : ""}
              {field.kind === "select" ? (
                <select
                  value={values[field.key]}
                  onChange={(event) => {
                    if (
                      field.key === templateSelector &&
                      Object.hasOwn(variants, event.target.value)
                    )
                      setSelectedVariant(event.target.value);
                    setTrial({ ...trial, [field.key]: event.target.value });
                  }}
                >
                  <option value="">请选择</option>
                  {field.options?.map((option) => (
                    <option key={option}>{option}</option>
                  ))}
                </select>
              ) : field.kind === "text" || field.kind === "number" ? (
                <input
                  type={field.kind === "number" ? "number" : "text"}
                  step="any"
                  min={field.min}
                  max={field.max}
                  required={field.required}
                  value={values[field.key]}
                  onChange={(event) =>
                    setTrial({ ...trial, [field.key]: event.target.value })
                  }
                />
              ) : (
                <small>在实际 Record 中配置{field.label}</small>
              )}
            </label>
          ))}
        <h4>正文效果</h4>
        <pre>{preview}</pre>
        <button type="button" onClick={() => setTrial({})}>
          重置试填
        </button>
      </aside>
    </div>
  );
}
