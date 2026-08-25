import { useMemo, useState } from "react";
import {
  createQpcrDeltaCtAnalysis,
  createQpcrDeltaDeltaCtAnalysis,
  uid,
  type AssayWorkspace,
} from "./repository";

type Props = {
  recordId: string;
  workspace: AssayWorkspace;
  refresh: () => Promise<void>;
};

const fixed = (value: number) =>
  Number.isFinite(value) ? value.toFixed(3) : "—";

export default function QpcrAnalysisWorkspace({
  recordId,
  workspace,
  refresh,
}: Props) {
  const numericWells = useMemo(
    () => workspace.joinedWells.filter((well) => well.numericValue != null),
    [workspace.joinedWells],
  );
  const [roles, setRoles] = useState<
    Record<string, "" | "target" | "reference">
  >({});
  const [excluded, setExcluded] = useState<Set<string>>(new Set());
  const [qcNotes, setQcNotes] = useState<Record<string, string>>({});
  const [deltaName, setDeltaName] = useState("ΔCt Analysis");
  const [sourceId, setSourceId] = useState("");
  const [referenceId, setReferenceId] = useState("");
  const [sampleGroups, setSampleGroups] = useState<Record<string, string>>({});
  const [controlMode, setControlMode] = useState<"shared" | "matched">(
    "shared",
  );
  const [sharedControl, setSharedControl] = useState("");
  const [controlRelations, setControlRelations] = useState<
    Record<string, string>
  >({});
  const [deltaDeltaName, setDeltaDeltaName] = useState("ΔΔCt Analysis");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [savedMessage, setSavedMessage] = useState("");

  const selectedSourceId =
    sourceId || workspace.deltaCtAnalyses.at(-1)?.id || "";
  const source = workspace.deltaCtAnalyses.find(
    (analysis) => analysis.id === selectedSourceId,
  );
  const selectedReferenceId = source?.config.referenceItemIds.includes(
    referenceId,
  )
    ? referenceId
    : source?.config.referenceItemIds[0] || "";

  const sourceSamples = useMemo(() => {
    const byId = new Map<string, string>();
    source?.result.combinations.forEach((combination) =>
      combination.samples.forEach((sample) =>
        byId.set(sample.sampleId, sample.sampleCode),
      ),
    );
    return [...byId].map(([id, code]) => ({ id, code }));
  }, [source]);
  const groups = [
    ...new Set(
      Object.values(sampleGroups)
        .map((group) => group.trim())
        .filter(Boolean),
    ),
  ];
  const itemName = (id: string) =>
    workspace.items.find((item) => item.id === id)?.displayName || id;

  const saveDeltaCt = async () => {
    const targetItemIds = Object.entries(roles)
      .filter(([, role]) => role === "target")
      .map(([id]) => id);
    const referenceItemIds = Object.entries(roles)
      .filter(([, role]) => role === "reference")
      .map(([id]) => id);
    setBusy(true);
    try {
      await createQpcrDeltaCtAnalysis({
        id: uid("delta-ct"),
        recordId,
        name: deltaName.trim() || "ΔCt Analysis",
        targetItemIds,
        referenceItemIds,
        includedMeasurementIds: numericWells
          .filter((well) => !excluded.has(well.measurementId))
          .map((well) => well.measurementId),
        qcNotes,
        createdAt: new Date().toISOString(),
      });
      await refresh();
      setError("");
      setSavedMessage("ΔCt 已保存，并已作为确切文本补充到 Record。");
    } catch (reason) {
      setSavedMessage("");
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setBusy(false);
    }
  };

  const saveDeltaDeltaCt = async () => {
    if (!source) return;
    setBusy(true);
    try {
      await createQpcrDeltaDeltaCtAnalysis({
        id: uid("delta-delta-ct"),
        recordId,
        deltaCtAnalysisId: source.id,
        name: deltaDeltaName.trim() || "ΔΔCt Analysis",
        referenceItemId: selectedReferenceId,
        controlMode,
        sampleGroups,
        sharedControlGroup: sharedControl,
        controlRelations,
        createdAt: new Date().toISOString(),
      });
      await refresh();
      setError("");
      setSavedMessage("ΔΔCt 已保存，并已作为确切文本补充到 Record。");
    } catch (reason) {
      setSavedMessage("");
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="qpcr-analysis">
      <header>
        <div>
          <b>4. qPCR Analysis</b>
          <small>
            分析快照独立保存，不改变 Raw Cq、Mapping 或 Sample lineage。
          </small>
        </div>
      </header>

      <section className="qpcr-analysis-step">
        <h3>第一层 · ΔCt</h3>
        <p>
          为基因分配角色；多个内参不会平均，而是分别形成 Target × 内参计算。
        </p>
        <div className="qpcr-role-grid">
          {workspace.items.map((item) => (
            <label key={item.id}>
              <span>{item.displayName}</span>
              <select
                value={roles[item.id] || ""}
                onChange={(event) =>
                  setRoles((current) => ({
                    ...current,
                    [item.id]: event.target.value as
                      "" | "target" | "reference",
                  }))
                }
              >
                <option value="">不参与</option>
                <option value="target">目的基因</option>
                <option value="reference">内参基因</option>
              </select>
            </label>
          ))}
        </div>
        <div className="qpcr-well-qc">
          <b>Well inclusion / QC</b>
          {numericWells.map((well) => (
            <div key={well.measurementId}>
              <label>
                <input
                  type="checkbox"
                  checked={!excluded.has(well.measurementId)}
                  onChange={(event) =>
                    setExcluded((current) => {
                      const next = new Set(current);
                      if (event.target.checked) next.delete(well.measurementId);
                      else next.add(well.measurementId);
                      return next;
                    })
                  }
                />
                {well.wellPosition} · {well.sampleCode} / {well.assayItem} ·{" "}
                {well.textValue}
              </label>
              {excluded.has(well.measurementId) && (
                <input
                  aria-label={`${well.wellPosition} 排除原因`}
                  placeholder="排除原因（可选）"
                  value={qcNotes[well.measurementId] || ""}
                  onChange={(event) =>
                    setQcNotes((current) => ({
                      ...current,
                      [well.measurementId]: event.target.value,
                    }))
                  }
                />
              )}
            </div>
          ))}
        </div>
        <div className="qpcr-action-row">
          <input
            value={deltaName}
            onChange={(event) => setDeltaName(event.target.value)}
          />
          <button
            className="primary"
            disabled={busy}
            onClick={() => void saveDeltaCt()}
          >
            保存 ΔCt Analysis
          </button>
        </div>
        {workspace.deltaCtAnalyses.map((analysis) => (
          <details className="qpcr-snapshot" key={analysis.id}>
            <summary>
              {analysis.name} ·{" "}
              {new Date(analysis.createdAt).toLocaleString("zh-CN")}
            </summary>
            {analysis.result.combinations.map((combination) => (
              <div
                key={`${combination.targetItemId}-${combination.referenceItemId}`}
              >
                <b>
                  {itemName(combination.targetItemId)} ×{" "}
                  {itemName(combination.referenceItemId)}
                </b>
                {combination.samples.map((sample) => (
                  <span key={sample.sampleId}>
                    {sample.sampleCode}: ΔCt {fixed(sample.deltaCt)}（
                    {sample.targetReplicateCount}/
                    {sample.referenceReplicateCount} wells）
                  </span>
                ))}
              </div>
            ))}
          </details>
        ))}
      </section>

      <section className="qpcr-analysis-step">
        <h3>第二层 · ΔΔCt（可选）</h3>
        <p>
          选择一个已保存的 ΔCt 分析和其中一个内参，再定义样本分组与对照关系。
        </p>
        <div className="qpcr-config-grid">
          <label>
            ΔCt 来源
            <select
              value={selectedSourceId}
              onChange={(event) => setSourceId(event.target.value)}
            >
              <option value="">请选择</option>
              {workspace.deltaCtAnalyses.map((analysis) => (
                <option value={analysis.id} key={analysis.id}>
                  {analysis.name}
                </option>
              ))}
            </select>
          </label>
          <label>
            本次内参
            <select
              value={selectedReferenceId}
              onChange={(event) => setReferenceId(event.target.value)}
              disabled={!source}
            >
              {(source?.config.referenceItemIds || []).map((id) => (
                <option value={id} key={id}>
                  {itemName(id)}
                </option>
              ))}
            </select>
          </label>
        </div>
        {source && (
          <>
            <div className="qpcr-sample-groups">
              {sourceSamples.map((sample) => (
                <label key={sample.id}>
                  <span>{sample.code}</span>
                  <input
                    placeholder="样本组，例如 siNC 24h"
                    value={sampleGroups[sample.id] || ""}
                    onChange={(event) =>
                      setSampleGroups((current) => ({
                        ...current,
                        [sample.id]: event.target.value,
                      }))
                    }
                  />
                </label>
              ))}
            </div>
            <div className="qpcr-control-config">
              <label>
                <input
                  type="radio"
                  checked={controlMode === "shared"}
                  onChange={() => setControlMode("shared")}
                />
                所有实验组共用一个对照组
              </label>
              <label>
                <input
                  type="radio"
                  checked={controlMode === "matched"}
                  onChange={() => setControlMode("matched")}
                />
                每个实验组使用对应对照组
              </label>
              {controlMode === "shared" ? (
                <select
                  value={sharedControl}
                  onChange={(event) => setSharedControl(event.target.value)}
                >
                  <option value="">选择共同对照组</option>
                  {groups.map((group) => (
                    <option key={group}>{group}</option>
                  ))}
                </select>
              ) : (
                groups.map((group) => (
                  <label key={group}>
                    <span>{group} →</span>
                    <select
                      value={controlRelations[group] || ""}
                      onChange={(event) =>
                        setControlRelations((current) => ({
                          ...current,
                          [group]: event.target.value,
                        }))
                      }
                    >
                      <option value="">选择对应对照</option>
                      {groups.map((candidate) => (
                        <option key={candidate}>{candidate}</option>
                      ))}
                    </select>
                  </label>
                ))
              )}
            </div>
            <div className="qpcr-action-row">
              <input
                value={deltaDeltaName}
                onChange={(event) => setDeltaDeltaName(event.target.value)}
              />
              <button
                className="primary"
                disabled={busy}
                onClick={() => void saveDeltaDeltaCt()}
              >
                保存 ΔΔCt Analysis
              </button>
            </div>
          </>
        )}
        {workspace.deltaDeltaCtAnalyses.map((analysis) => (
          <details className="qpcr-snapshot" key={analysis.id}>
            <summary>
              {analysis.name} ·{" "}
              {new Date(analysis.createdAt).toLocaleString("zh-CN")}
            </summary>
            {analysis.result.combinations.map((combination) => (
              <div key={`${analysis.id}-${combination.targetItemId}`}>
                <b>
                  {itemName(combination.targetItemId)} /{" "}
                  {itemName(combination.referenceItemId)}
                </b>
                {combination.samples.map((sample) => (
                  <span key={sample.sampleId}>
                    {sample.sampleCode} · {sample.group} vs{" "}
                    {sample.controlGroup}: 2^-ΔΔCt ={" "}
                    {fixed(sample.relativeExpression)}
                  </span>
                ))}
              </div>
            ))}
          </details>
        ))}
      </section>
      {savedMessage && <p className="assay-save-success">{savedMessage}</p>}
      {error && <p className="form-error">{error}</p>}
    </section>
  );
}
