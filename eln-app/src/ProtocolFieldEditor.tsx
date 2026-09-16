import type { ProtocolField } from "./domain";
import { validProtocolFieldKey } from "./protocolFieldKey";
import { useComposerDrag } from "./useComposerDrag";

const editableKinds = ["text", "number", "select"] as const;

export function ProtocolFieldEditor({
  fields,
  onChange,
  lockedKeys = [],
}: {
  fields: ProtocolField[];
  onChange: (fields: ProtocolField[]) => void;
  lockedKeys?: string[];
}) {
  const update = (index: number, patch: Partial<ProtocolField>) =>
    onChange(
      fields.map((field, itemIndex) =>
        itemIndex === index ? { ...field, ...patch } : field,
      ),
    );
  const add = () => {
    let index = fields.length + 1;
    while (fields.some((field) => field.key === `field_${index}`)) index += 1;
    onChange([
      ...fields,
      { key: `field_${index}`, label: "新字段", kind: "text" },
    ]);
  };
  const changeKind = (index: number, kind: ProtocolField["kind"]) => {
    const field = fields[index];
    if (kind === "number") update(index, { kind, options: undefined });
    else
      update(index, {
        kind,
        options: kind === "select" ? field.options || [] : undefined,
        unit: undefined,
        min: undefined,
        max: undefined,
      });
  };

  const fieldDrag = useComposerDrag((raw, target) => {
    const to = target
      .closest("[data-field-index]")
      ?.getAttribute("data-field-index");
    const from = Number(raw);
    if (
      to == null ||
      !Number.isInteger(from) ||
      from < 0 ||
      from >= fields.length
    )
      return;
    const next = [...fields];
    const [moved] = next.splice(from, 1);
    next.splice(Number(to), 0, moved);
    onChange(next);
  });
  return (
    <section className="protocol-field-editor">
      <div className="protocol-field-heading">
        <div>
          <h3>Record 字段</h3>
          <p>字段会在创建 Record 时显示；无需编辑 JSON。</p>
        </div>
        <button className="secondary" type="button" onClick={add}>
          ＋ 添加字段
        </button>
      </div>
      <div className="field-editor-guide">
        <strong>怎么填写？</strong>
        <span>
          每个字段就是 Record 里的一项记录。<b>Key</b>{" "}
          是正文占位符使用的内部名称（例如
          <code>dose</code>），<b>标签</b>{" "}
          是实验人员看到的名称（例如“刺激浓度”），可以使用中文。类型决定输入方式；默认值会自动预填，勾选“必填”后不能留空。
        </span>
        <span>
          <b>显示条件（可选）</b>
          ：在“什么时候显示”选择另一个字段，再选择或填写它的值。例如选择“处理类型”和“药物”，浓度就只在药物处理时出现。选择“始终显示”可取消此条件。
        </span>
      </div>
      {fields.length === 0 && <p className="field-empty">暂无自定义字段。</p>}
      {fields.map((field, index) => {
        const locked = lockedKeys.includes(field.key);
        const visibleWhen = field.visibleWhen;
        return (
          <article
            className="protocol-field-card"
            key={index}
            data-field-index={index}
            onDragOver={(event) => event.preventDefault()}
            onDrop={(event) => {
              const raw = event.dataTransfer.getData(
                "application/labflow-field-index",
              );
              if (!raw) return;
              event.preventDefault();
              event.stopPropagation();
              const from = Number(raw);
              if (!Number.isInteger(from) || from < 0 || from >= fields.length)
                return;
              const next = [...fields];
              const [moved] = next.splice(from, 1);
              next.splice(index, 0, moved);
              onChange(next);
            }}
          >
            <div className="module-actions">
              <strong>{field.label || "新字段"}</strong>
              <button
                className="field-remove"
                type="button"
                disabled={locked}
                title={locked ? "来源执行所需字段，不能删除" : "删除此字段"}
                onClick={() =>
                  onChange(fields.filter((_, itemIndex) => itemIndex !== index))
                }
              >
                {locked ? "来源字段（不可删除）" : "删除字段"}
              </button>
              <button
                type="button"
                className="drag-handle"
                {...fieldDrag(String(index))}
              >
                ⠿ 拖动排序
              </button>
              <button
                type="button"
                disabled={index === 0}
                onClick={() => {
                  const next = [...fields];
                  [next[index - 1], next[index]] = [
                    next[index],
                    next[index - 1],
                  ];
                  onChange(next);
                }}
              >
                上移
              </button>
              <button
                type="button"
                disabled={index === fields.length - 1}
                onClick={() => {
                  const next = [...fields];
                  [next[index + 1], next[index]] = [
                    next[index],
                    next[index + 1],
                  ];
                  onChange(next);
                }}
              >
                下移
              </button>
            </div>
            <div className="field-card-top">
              <label>
                Key（正文占位符）
                {locked ? (
                  <code className="locked-placeholder">{`{{${field.key}}}`}</code>
                ) : (
                  <input
                    value={field.key}
                    className={
                      validProtocolFieldKey(field.key) ? "" : "invalid"
                    }
                    placeholder="例如 treatment_dose"
                    aria-invalid={!validProtocolFieldKey(field.key)}
                    onChange={(event) =>
                      update(index, { key: event.target.value.trim() })
                    }
                  />
                )}
                {!locked && (
                  <small
                    className={
                      validProtocolFieldKey(field.key)
                        ? "key-help"
                        : "key-help invalid-help"
                    }
                  >
                    仅限英文字母、数字和下划线，必须以字母或下划线开头，最多 64
                    个字符。正文占位符：
                    {validProtocolFieldKey(field.key)
                      ? `{{${field.key}}}`
                      : "Key 格式正确后显示"}
                  </small>
                )}
              </label>
              <label>
                标签（Record 中显示）
                <input
                  value={field.label}
                  onChange={(event) =>
                    update(index, { label: event.target.value })
                  }
                />
              </label>
              <label>
                类型
                {locked ? (
                  <span className="locked-field-kind">
                    {
                      {
                        text: "文字",
                        number: "数字",
                        select: "下拉选项",
                        samples: "Sample",
                        plate_layout: "孔板布局",
                        condition_groups: "条件分组",
                      }[field.kind]
                    }
                  </span>
                ) : (
                  <select
                    value={field.kind}
                    disabled={
                      !editableKinds.includes(
                        field.kind as (typeof editableKinds)[number],
                      )
                    }
                    onChange={(event) =>
                      changeKind(
                        index,
                        event.target.value as ProtocolField["kind"],
                      )
                    }
                  >
                    {!editableKinds.includes(
                      field.kind as (typeof editableKinds)[number],
                    ) && <option value={field.kind}>{field.kind}</option>}
                    {editableKinds.map((kind) => (
                      <option key={kind} value={kind}>
                        {
                          { text: "文字", number: "数字", select: "下拉选项" }[
                            kind
                          ]
                        }
                      </option>
                    ))}
                  </select>
                )}
              </label>
            </div>
            {locked && (
              <small>
                此占位符由来源 Protocol
                的执行规则使用，固定为只读；可调整标签、默认值和正文说明。
              </small>
            )}
            <div className="field-options-grid">
              <label className="checkbox-label">
                <input
                  type="checkbox"
                  disabled={locked}
                  checked={Boolean(field.required)}
                  onChange={(event) =>
                    update(index, {
                      required: event.target.checked || undefined,
                    })
                  }
                />
                必填
              </label>
              {editableKinds.includes(
                field.kind as (typeof editableKinds)[number],
              ) && (
                <label>
                  默认值
                  <input
                    value={field.defaultValue || ""}
                    onChange={(event) =>
                      update(index, {
                        defaultValue: event.target.value || undefined,
                      })
                    }
                  />
                </label>
              )}
              {field.kind === "select" && (
                <label className="field-wide">
                  选项（每行一个）
                  <textarea
                    rows={3}
                    disabled={locked}
                    value={(field.options || []).join("\n")}
                    onChange={(event) =>
                      update(index, {
                        options: event.target.value.split("\n"),
                      })
                    }
                  />
                </label>
              )}
              {field.kind === "number" && (
                <>
                  <label>
                    单位
                    <input
                      value={field.unit || ""}
                      onChange={(event) =>
                        update(index, { unit: event.target.value || undefined })
                      }
                    />
                  </label>
                  <label>
                    最小值
                    <input
                      type="number"
                      value={field.min ?? ""}
                      onChange={(event) =>
                        update(index, {
                          min:
                            event.target.value === ""
                              ? undefined
                              : Number(event.target.value),
                        })
                      }
                    />
                  </label>
                  <label>
                    最大值
                    <input
                      type="number"
                      value={field.max ?? ""}
                      onChange={(event) =>
                        update(index, {
                          max:
                            event.target.value === ""
                              ? undefined
                              : Number(event.target.value),
                        })
                      }
                    />
                  </label>
                </>
              )}
              <label>
                什么时候显示
                <select
                  value={visibleWhen?.key || ""}
                  disabled={locked}
                  onChange={(event) =>
                    update(index, {
                      visibleWhen: event.target.value
                        ? {
                            key: event.target.value,
                            value: "",
                          }
                        : undefined,
                    })
                  }
                >
                  <option value="">始终显示</option>
                  {fields
                    .filter(
                      (item) =>
                        item.key !== field.key &&
                        editableKinds.includes(
                          item.kind as (typeof editableKinds)[number],
                        ),
                    )
                    .map((item) => (
                      <option key={item.key} value={item.key}>
                        当「{item.label}」的值等于…
                      </option>
                    ))}
                </select>
              </label>
              <label>
                等于以下值时显示
                {fields.find((item) => item.key === visibleWhen?.key)?.kind ===
                "select" ? (
                  <select
                    disabled={locked || !visibleWhen}
                    value={visibleWhen?.value || ""}
                    onChange={(event) =>
                      update(index, {
                        visibleWhen: visibleWhen
                          ? { ...visibleWhen, value: event.target.value }
                          : undefined,
                      })
                    }
                  >
                    <option value="">请选择</option>
                    {fields
                      .find((item) => item.key === visibleWhen?.key)
                      ?.options?.map((option) => (
                        <option key={option} value={option}>
                          {option}
                        </option>
                      ))}
                  </select>
                ) : (
                  <input
                    value={visibleWhen?.value || ""}
                    disabled={locked || !visibleWhen}
                    onChange={(event) =>
                      update(index, {
                        visibleWhen: visibleWhen
                          ? { ...visibleWhen, value: event.target.value }
                          : undefined,
                      })
                    }
                  />
                )}
              </label>
            </div>
          </article>
        );
      })}
    </section>
  );
}
