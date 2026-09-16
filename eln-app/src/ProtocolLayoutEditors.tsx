export type PlateTreatmentGroup = {
  factor: string;
  duration: string;
  wellCount: number;
};

export type SampleConditionGroup = {
  condition: string;
  dose: string;
  duration: string;
  method: string;
  sampleCount: number;
};

export function PlateLayoutEditor({
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
      groups.map((group, i) => (i === index ? { ...group, ...patch } : group)),
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

export function ConditionGroupEditor({
  capacity,
  plateMapping,
  containerMode = plateMapping ? "plate" : "independent",
  groups,
  onChange,
}: {
  capacity: number;
  plateMapping: boolean;
  containerMode?: "independent" | "plate" | "dish";
  groups: SampleConditionGroup[];
  onChange: (groups: SampleConditionGroup[]) => void;
}) {
  const used = groups.reduce((total, group) => total + group.sampleCount, 0);
  const update = (index: number, patch: Partial<SampleConditionGroup>) =>
    onChange(
      groups.map((group, i) => (i === index ? { ...group, ...patch } : group)),
    );
  return (
    <div className="condition-layout">
      <div
        className={`plate-capacity ${plateMapping && capacity > 0 && used > capacity ? "over" : ""}`}
      >
        <b>
          {containerMode === "plate"
            ? `${capacity || "?"} 孔板`
            : containerMode === "dish"
              ? "培养皿分配"
              : "独立容器"}
        </b>
        <span>
          每个输入产生 {used} 个 Sample
          {containerMode === "plate"
            ? ` · 已分配 ${used} / ${capacity || "?"} 孔`
            : containerMode === "dish"
              ? ` · 已分配 ${used} 个皿`
              : ""}
        </span>
      </div>
      <div className="condition-group condition-group-head" aria-hidden="true">
        <span />
        <b>实验条件（必填）</b>
        <b>浓度（可选）</b>
        <b>处理时间（可选）</b>
        <b>处理方法（可选）</b>
        <b>数量</b>
        <span />
      </div>
      {groups.map((group, index) => (
        <div className="condition-group" key={index}>
          <span>{index + 1}</span>
          <input
            aria-label={`第 ${index + 1} 组实验条件`}
            placeholder="例如 Control 或 TNF-α"
            value={group.condition}
            onChange={(event) =>
              update(index, { condition: event.target.value })
            }
          />
          <input
            aria-label={`第 ${index + 1} 组浓度`}
            placeholder="例如 10 ng/mL"
            value={group.dose}
            onChange={(event) => update(index, { dose: event.target.value })}
          />
          <input
            aria-label={`第 ${index + 1} 组处理时间`}
            placeholder="例如 24 h"
            value={group.duration}
            onChange={(event) =>
              update(index, { duration: event.target.value })
            }
          />
          <input
            aria-label={`第 ${index + 1} 组处理方法`}
            placeholder="例如换液后加药"
            value={group.method}
            onChange={(event) => update(index, { method: event.target.value })}
          />
          <input
            aria-label={`第 ${index + 1} 组 Sample 数量`}
            type="number"
            min="1"
            max={plateMapping ? capacity || 384 : 384}
            value={group.sampleCount}
            onChange={(event) =>
              update(index, { sampleCount: Number(event.target.value) })
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
          onChange([
            ...groups,
            {
              condition: "",
              dose: "",
              duration: "",
              method: "",
              sampleCount: 1,
            },
          ])
        }
      >
        ＋ 增加条件组
      </button>
    </div>
  );
}
