import { useEffect, useMemo, useState } from "react";
import type { RecordItem, Sample, TerminalAssayDefinition } from "./domain";
import {
  createAssayPlate,
  deleteEmptyAssayPlate,
  getAssayWorkspace,
  replaceAssayPlateMappings,
  uid,
  uploadAssayRawFile,
  type AssayWorkspace,
} from "./repository";
import QpcrAnalysisWorkspace from "./QpcrAnalysisWorkspace";
import "./assay-mapping.css";

type Props = {
  record: RecordItem;
  samples: Sample[];
  definition: TerminalAssayDefinition;
  changed: () => void;
};

type DraftMapping = {
  id?: string;
  sampleId: string;
  assayItemId: string;
};

const draftForPlate = (workspace: AssayWorkspace, plateId: string) =>
  Object.fromEntries(
    workspace.mappings
      .filter((mapping) => mapping.plateId === plateId)
      .map((mapping) => [
        mapping.wellPosition,
        {
          id: mapping.id,
          sampleId: mapping.sampleId,
          assayItemId: mapping.assayItemId,
        },
      ]),
  ) as Record<string, DraftMapping>;

const capacity = (model: string) => Number(model);

const dimensions = (model: string) => {
  const value = capacity(model);
  return (
    {
      6: [2, 3],
      12: [3, 4],
      24: [4, 6],
      48: [6, 8],
      96: [8, 12],
      384: [16, 24],
    } as Record<number, [number, number]>
  )[value];
};

const wellPositions = (model: string) => {
  const [rows, columns] = dimensions(model) || [0, 0];
  return Array.from({ length: rows }, (_, row) =>
    Array.from(
      { length: columns },
      (_, column) =>
        `${String.fromCharCode("A".charCodeAt(0) + row)}${String(column + 1).padStart(2, "0")}`,
    ),
  ).flat();
};

const parseHeader = (text: string) => {
  const line = text.replace(/^\uFEFF/, "").split(/\r?\n/, 1)[0] || "";
  const delimiter =
    (line.match(/\t/g)?.length || 0) > (line.match(/,/g)?.length || 0)
      ? "\t"
      : ",";
  const fields: string[] = [];
  let value = "";
  let quoted = false;
  for (let index = 0; index < line.length; index += 1) {
    const character = line[index];
    if (character === '"' && quoted && line[index + 1] === '"') {
      value += '"';
      index += 1;
    } else if (character === '"') quoted = !quoted;
    else if (character === delimiter && !quoted) {
      fields.push(value.trim());
      value = "";
    } else value += character;
  }
  fields.push(value.trim());
  return fields.filter(Boolean);
};

export default function TerminalAssayWorkspace({
  record,
  samples,
  definition,
  changed,
}: Props) {
  const [workspace, setWorkspace] = useState<AssayWorkspace>();
  const [selectedPlateId, setSelectedPlateId] = useState("");
  const [plateName, setPlateName] = useState("Plate 1");
  const [plateModel, setPlateModel] = useState(
    definition.plateModels[0] || "96",
  );
  const [selectedOption, setSelectedOption] = useState("");
  const [draft, setDraft] = useState<Record<string, DraftMapping>>({});
  const [file, setFile] = useState<File>();
  const [headers, setHeaders] = useState<string[]>([]);
  const [wellColumn, setWellColumn] = useState("");
  const [measurementColumn, setMeasurementColumn] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [analysisOpen, setAnalysisOpen] = useState(false);
  const [step, setStep] = useState(1);

  const refresh = async (preferredPlate?: string) => {
    const next = await getAssayWorkspace(record.id);
    const nextPlate = next.plates.some(
      (plate) => plate.id === (preferredPlate || selectedPlateId),
    )
      ? preferredPlate || selectedPlateId
      : next.plates[0]?.id || "";
    setWorkspace(next);
    setSelectedPlateId(nextPlate);
    setDraft(draftForPlate(next, nextPlate));
  };

  useEffect(() => {
    let cancelled = false;
    void getAssayWorkspace(record.id)
      .then((next) => {
        if (cancelled) return;
        const firstPlate = next.plates[0]?.id || "";
        setWorkspace(next);
        setSelectedPlateId(firstPlate);
        setDraft(draftForPlate(next, firstPlate));
      })
      .catch((reason) => {
        if (!cancelled)
          setError(reason instanceof Error ? reason.message : String(reason));
      });
    return () => {
      cancelled = true;
    };
  }, [record.id]);

  const selectedPlate = workspace?.plates.find(
    (plate) => plate.id === selectedPlateId,
  );
  const options = useMemo(
    () =>
      record.inputs.flatMap((sampleId) => {
        const sample = samples.find((candidate) => candidate.id === sampleId);
        return (workspace?.items || []).map((item) => ({
          sampleId,
          assayItemId: item.id,
          label: `${sample?.code || sampleId} / ${item.displayName}`,
        }));
      }),
    [record.inputs, samples, workspace?.items],
  );

  const selectedAssignment =
    selectedOption === "" ? undefined : options[Number(selectedOption)];
  const labelFor = (mapping?: DraftMapping) =>
    mapping
      ? options.find(
          (option) =>
            option.sampleId === mapping.sampleId &&
            option.assayItemId === mapping.assayItemId,
        )?.label
      : undefined;
  const replicateCounts = Object.values(draft).reduce<Record<string, number>>(
    (counts, mapping) => {
      const label = labelFor(mapping) || "未知映射";
      counts[label] = (counts[label] || 0) + 1;
      return counts;
    },
    {},
  );

  const createPlate = async () => {
    if (!plateName.trim()) return setError("请填写板名称。");
    const id = uid("assay-plate");
    setBusy(true);
    try {
      await createAssayPlate({
        id,
        recordId: record.id,
        name: plateName.trim(),
        plateModel,
        createdAt: new Date().toISOString(),
      });
      await refresh(id);
      setPlateName(`Plate ${(workspace?.plates.length || 0) + 2}`);
      setError("");
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setBusy(false);
    }
  };

  const saveMapping = async () => {
    if (!selectedPlate) return;
    setBusy(true);
    try {
      await replaceAssayPlateMappings(
        selectedPlate.id,
        Object.entries(draft).map(([wellPosition, mapping]) => ({
          id: mapping.id || uid("assay-map"),
          wellPosition,
          sampleId: mapping.sampleId,
          assayItemId: mapping.assayItemId,
        })),
      );
      await refresh(selectedPlate.id);
      setError("");
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setBusy(false);
    }
  };

  const deletePlate = async () => {
    if (!selectedPlate) return;
    if (!window.confirm(`删除空白板“${selectedPlate.name}”？`)) return;
    setBusy(true);
    try {
      await deleteEmptyAssayPlate(selectedPlate.id);
      await refresh();
      setError("");
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setBusy(false);
    }
  };

  const chooseFile = async (next?: File) => {
    setFile(next);
    setError("");
    if (!next) {
      setHeaders([]);
      return;
    }
    const parsedHeaders = next.name.toLowerCase().endsWith(".xlsx")
      ? ["Well", definition.metricLabel]
      : parseHeader(await next.text());
    setHeaders(parsedHeaders);
    const automaticWell =
      parsedHeaders.find((header) =>
        /^(well|well position|position|孔位)$/i.test(header),
      ) ||
      parsedHeaders[0] ||
      "";
    const metricTokens = [definition.metricKey, definition.metricLabel]
      .map((token) => token.toLowerCase())
      .filter(Boolean);
    const automaticMeasurement =
      parsedHeaders.find((header) =>
        metricTokens.some((token) => header.toLowerCase().includes(token)),
      ) ||
      parsedHeaders.find((header) => header !== automaticWell) ||
      "";
    setWellColumn(automaticWell);
    setMeasurementColumn(automaticMeasurement);
  };

  const upload = async () => {
    if (!selectedPlate || !file || !wellColumn || !measurementColumn)
      return setError("请选择 Raw 文件、孔位列和测量值列。");
    setBusy(true);
    try {
      const attachmentId = uid("attachment");
      await uploadAssayRawFile(
        {
          id: uid("raw-import"),
          recordId: record.id,
          plateId: selectedPlate.id,
          attachmentId,
          fileName: file.name,
          mimeType: file.type || "text/plain",
          metricKey: definition.metricKey,
          wellColumn,
          measurementColumn,
          importedAt: new Date().toISOString(),
        },
        Array.from(new Uint8Array(await file.arrayBuffer())),
      );
      await refresh(selectedPlate.id);
      changed();
      setFile(undefined);
      setHeaders([]);
      setError("");
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setBusy(false);
    }
  };

  if (!workspace) return <p>正在读取 Plate Mapping…</p>;
  if (workspace.items.length === 0)
    return (
      <section className="terminal-assay">
        <h2>Plate Mapping & Raw Data</h2>
        <p className="assay-empty">
          该历史 Record
          创建时尚未保存通用检测项目，保持原记录不变；新建的终末检测 Record
          将启用 Mapping。
        </p>
      </section>
    );
  const plateJoined = workspace.joinedWells.filter(
    (row) => row.plateId === selectedPlateId,
  );
  const plateImports = workspace.imports.filter(
    (item) => item.plateId === selectedPlateId,
  );
  const isQpcr = definition.metricKey.toLowerCase() === "cq";
  const selectedPlateIsEmpty =
    !!selectedPlate &&
    workspace.mappings.every(
      (mapping) => mapping.plateId !== selectedPlate.id,
    ) &&
    workspace.imports.every((item) => item.plateId !== selectedPlate.id) &&
    Object.keys(draft).length === 0;
  const hasAnalysisProgress =
    workspace.plates.length > 0 ||
    workspace.imports.length > 0 ||
    workspace.deltaCtAnalyses.length > 0 ||
    workspace.deltaDeltaCtAnalyses.length > 0;

  if (isQpcr && !analysisOpen)
    return (
      <section className="terminal-assay assay-launcher">
        <div>
          <h2>qPCR 数据分析</h2>
          <p>
            Plate Mapping、Raw Cq、Join Dataset 与 ΔCt / ΔΔCt
            在独立分析工作区中按步骤完成。
          </p>
        </div>
        <button
          className="primary"
          onClick={() => {
            setStep(1);
            setAnalysisOpen(true);
          }}
        >
          {hasAnalysisProgress ? "继续分析" : "开始分析"}
        </button>
      </section>
    );

  return (
    <section className={`terminal-assay ${isQpcr ? "analysis-open" : ""}`}>
      <div className="terminal-assay-title">
        <div>
          <h2>Plate Mapping & Raw Data</h2>
          <p>
            Mapping 是独立层；每个孔指向 Sample × {definition.itemLabel}
            ，不会创建新 Sample。
          </p>
        </div>
        {isQpcr ? (
          <button
            className="secondary assay-exit"
            onClick={() => setAnalysisOpen(false)}
          >
            退出分析
          </button>
        ) : (
          <span>Mapping + Raw</span>
        )}
      </div>

      {isQpcr && (
        <nav className="assay-step-nav" aria-label="qPCR 分析步骤">
          {["Plate Mapping", "Raw Result", "Mapped Join", "qPCR Analysis"].map(
            (label, index) => (
              <button
                className={step === index + 1 ? "active" : ""}
                onClick={() => setStep(index + 1)}
                key={label}
              >
                <b>{index + 1}</b>
                {label}
              </button>
            ),
          )}
        </nav>
      )}

      {(!isQpcr || step === 1) && (
        <div className="assay-create-plate">
          <input
            aria-label="板名称"
            value={plateName}
            onChange={(event) => setPlateName(event.target.value)}
          />
          <select
            aria-label="板型"
            value={plateModel}
            onChange={(event) => setPlateModel(event.target.value)}
          >
            {definition.plateModels.map((model) => (
              <option value={model} key={model}>
                {model} 孔板
              </option>
            ))}
          </select>
          <button
            className="secondary"
            disabled={busy}
            onClick={() => void createPlate()}
          >
            新建板
          </button>
        </div>
      )}

      {(!isQpcr || step <= 3) && workspace.plates.length > 0 && (
        <div className="assay-plate-tabs">
          {workspace.plates.map((plate) => (
            <button
              className={plate.id === selectedPlateId ? "active" : ""}
              onClick={() => {
                setSelectedPlateId(plate.id);
                setDraft(draftForPlate(workspace, plate.id));
              }}
              key={plate.id}
            >
              {plate.name} · {plate.plateModel}孔
            </button>
          ))}
          {selectedPlate && (!isQpcr || step === 1) && (
            <button
              className="assay-delete-plate"
              disabled={busy || !selectedPlateIsEmpty}
              title={
                selectedPlateIsEmpty
                  ? "删除当前空白板"
                  : "只有没有 Mapping 和 Raw Result 的空白板可以删除"
              }
              onClick={() => void deletePlate()}
            >
              删除空白板
            </button>
          )}
        </div>
      )}

      {selectedPlate && (
        <>
          {(!isQpcr || step === 1) && (
            <section className="assay-mapping-layer">
              <header>
                <div>
                  <b>1. Plate Mapping</b>
                  <small>选择组合后点击孔位；再次点击相同组合可清空。</small>
                </div>
                <select
                  aria-label="Sample 与检测项目组合"
                  value={selectedOption}
                  onChange={(event) => setSelectedOption(event.target.value)}
                >
                  <option value="">选择 Sample × {definition.itemLabel}</option>
                  {options.map((option, index) => (
                    <option
                      value={index}
                      key={`${option.sampleId}-${option.assayItemId}`}
                    >
                      {option.label}
                    </option>
                  ))}
                </select>
              </header>
              <div className="assay-plate-scroll">
                <div
                  className="assay-plate-grid"
                  style={{
                    gridTemplateColumns: `repeat(${dimensions(selectedPlate.plateModel)?.[1] || 1}, minmax(64px, 1fr))`,
                  }}
                >
                  {wellPositions(selectedPlate.plateModel).map((well) => {
                    const mapping = draft[well];
                    const label = labelFor(mapping);
                    return (
                      <button
                        className={mapping ? "mapped" : ""}
                        title={label || well}
                        onClick={() => {
                          if (!selectedAssignment)
                            return setError(
                              "请先选择一个 Sample × 检测项目组合。",
                            );
                          setDraft((current) => {
                            const next = { ...current };
                            if (
                              mapping?.sampleId ===
                                selectedAssignment.sampleId &&
                              mapping?.assayItemId ===
                                selectedAssignment.assayItemId
                            )
                              delete next[well];
                            else
                              next[well] = {
                                sampleId: selectedAssignment.sampleId,
                                assayItemId: selectedAssignment.assayItemId,
                              };
                            return next;
                          });
                          setError("");
                        }}
                        key={well}
                      >
                        <b>{well}</b>
                        <span>{label || "未映射"}</span>
                      </button>
                    );
                  })}
                </div>
              </div>
              <div className="assay-mapping-summary">
                <div>
                  {Object.entries(replicateCounts).map(([label, count]) => (
                    <span key={label}>
                      {label} · {count}孔
                    </span>
                  ))}
                  {Object.keys(replicateCounts).length === 0 && (
                    <span>尚未分配孔位</span>
                  )}
                </div>
                <button
                  className="primary"
                  disabled={busy}
                  onClick={() => void saveMapping()}
                >
                  保存 Mapping
                </button>
              </div>
            </section>
          )}

          {(!isQpcr || step === 2) && (
            <section className="assay-raw-layer">
              <header>
                <div>
                  <b>2. Raw Result Attachment</b>
                  <small>
                    支持 UTF-8 CSV / TSV；qPCR 也支持 CFX 导出的
                    XLSX。原文件与解析值一并保存在用户数据目录。
                  </small>
                </div>
              </header>
              <div className="assay-upload-row">
                <input
                  aria-label="选择 Raw Result 文件"
                  type="file"
                  accept=".csv,.tsv,.txt,.xlsx,text/csv,text/tab-separated-values,text/plain,application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
                  onChange={(event) => void chooseFile(event.target.files?.[0])}
                />
                {headers.length > 0 && (
                  <>
                    <label>
                      孔位列
                      <select
                        value={wellColumn}
                        onChange={(event) => setWellColumn(event.target.value)}
                      >
                        {headers.map((header) => (
                          <option key={header}>{header}</option>
                        ))}
                      </select>
                    </label>
                    <label>
                      {definition.metricLabel} 列
                      <select
                        value={measurementColumn}
                        onChange={(event) =>
                          setMeasurementColumn(event.target.value)
                        }
                      >
                        {headers.map((header) => (
                          <option key={header}>{header}</option>
                        ))}
                      </select>
                    </label>
                    <button
                      className="primary"
                      disabled={busy}
                      onClick={() => void upload()}
                    >
                      保存并解析
                    </button>
                  </>
                )}
              </div>
              {plateImports.map((item) => (
                <p className="assay-import" key={item.id}>
                  <b>{item.fileName}</b> · {item.measurementCount} 条{" "}
                  {item.metricKey} · SHA-256 {item.contentSha256.slice(0, 12)}…
                </p>
              ))}
            </section>
          )}

          {(!isQpcr || step === 3) && (
            <section className="assay-join-layer">
              <header>
                <div>
                  <b>3. Mapped Join Dataset</b>
                  <small>
                    只显示同时存在 Mapping 和 Raw Measurement 的孔。
                  </small>
                </div>
                <span>{plateJoined.length} 条</span>
              </header>
              <div className="assay-join-table">
                <table>
                  <thead>
                    <tr>
                      <th>Well</th>
                      <th>Sample</th>
                      <th>{definition.itemLabel}</th>
                      <th>Metric</th>
                      <th>Value</th>
                      <th>Raw file</th>
                    </tr>
                  </thead>
                  <tbody>
                    {plateJoined.map((row) => (
                      <tr key={`${row.mappingId}-${row.measurementId}`}>
                        <td>{row.wellPosition}</td>
                        <td>{row.sampleCode}</td>
                        <td>{row.assayItem}</td>
                        <td>{row.metricKey}</td>
                        <td>{row.textValue}</td>
                        <td>{row.fileName}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
                {plateJoined.length === 0 && (
                  <p>Mapping 与 Raw Result 尚无相同孔位。</p>
                )}
              </div>
            </section>
          )}
        </>
      )}
      {!selectedPlate && (!isQpcr || step <= 3) && (
        <p className="assay-empty">
          先新建一块板，再进行 Mapping 或上传 Raw Result。
        </p>
      )}
      {isQpcr && step === 4 && (
        <QpcrAnalysisWorkspace
          recordId={record.id}
          workspace={workspace}
          refresh={async () => {
            await refresh(selectedPlateId);
            changed();
          }}
        />
      )}
      {isQpcr && (
        <footer className="assay-step-footer">
          <button
            className="secondary"
            disabled={step === 1}
            onClick={() => setStep((current) => Math.max(1, current - 1))}
          >
            上一步
          </button>
          <span>步骤 {step} / 4</span>
          <button
            className="primary"
            disabled={step === 4}
            onClick={() => setStep((current) => Math.min(4, current + 1))}
          >
            下一步
          </button>
        </footer>
      )}
      {error && <p className="form-error">{error}</p>}
    </section>
  );
}
