import type { Protocol, ProtocolField } from "./domain";
import { normalizeSampleType } from "./domain";
import type { Store } from "./repository";
import { isProtocolFieldVisible } from "./protocolFieldVisibility";
import {
  ConditionGroupEditor,
  PlateLayoutEditor,
  type PlateTreatmentGroup,
  type SampleConditionGroup,
} from "./ProtocolLayoutEditors";

export function ProtocolFields({
  field,
  protocol,
  samples,
  taskExperimentId,
  values,
  inputTypes,
  plateCapacity,
  conditionPlateCapacity,
  plateMapping,
  plateGroups,
  conditionGroups,
  setValue,
  setPlateGroups,
  setConditionGroups,
}: {
  field: ProtocolField;
  protocol: Protocol;
  samples: Store["samples"];
  taskExperimentId: string;
  values: Record<string, string>;
  inputTypes: string[];
  plateCapacity: number;
  conditionPlateCapacity: number;
  plateMapping: boolean;
  plateGroups: PlateTreatmentGroup[];
  conditionGroups: SampleConditionGroup[];
  setValue: (key: string, value: string) => void;
  setPlateGroups: (groups: PlateTreatmentGroup[]) => void;
  setConditionGroups: (groups: SampleConditionGroup[]) => void;
}) {
  if (!isProtocolFieldVisible(field, values, inputTypes)) return null;
  if (field.kind === "plate_layout")
    return (
      <div className="task-form">
        <span>{field.label}</span>
        <PlateLayoutEditor
          capacity={plateCapacity}
          groups={plateGroups}
          onChange={setPlateGroups}
        />
      </div>
    );
  if (field.kind === "condition_groups")
    return (
      <div className="task-form">
        <span>{field.label}</span>
        <ConditionGroupEditor
          capacity={conditionPlateCapacity}
          plateMapping={plateMapping}
          containerMode={protocol.execution?.conditionAllocation?.containerMode}
          groups={conditionGroups}
          onChange={setConditionGroups}
        />
      </div>
    );
  const control =
    field.kind === "samples" ? (
      <select
        value={values[field.key] || ""}
        onChange={(e) => setValue(field.key, e.target.value)}
      >
        <option value="">新建对象（不选择现有样本）</option>
        {samples
          .filter(
            (sample) =>
              !sample.consumed &&
              sample.experimentId === taskExperimentId &&
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
        onChange={(e) => setValue(field.key, e.target.value)}
      >
        <option value="">请选择</option>
        {field.options?.map((option) => (
          <option key={option}>{option}</option>
        ))}
      </select>
    ) : (
      <input
        type={field.kind === "number" ? "number" : "text"}
        step={field.kind === "number" ? "any" : undefined}
        min={field.kind === "number" ? field.min : undefined}
        max={field.kind === "number" ? field.max : undefined}
        value={values[field.key] || ""}
        onChange={(e) => setValue(field.key, e.target.value)}
      />
    );
  return (
    <label className="task-form">
      {field.label}
      {field.unit ? (
        <small className="field-unit">（{field.unit}）</small>
      ) : null}
      {control}
    </label>
  );
}
