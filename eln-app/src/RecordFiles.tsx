import { useEffect, useRef, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import type { RecordAttachment, RecordItem } from "./domain";
import {
  chooseRecordFiles,
  insertRecordFiles,
  openRecordAttachment,
  saveRecordAttachmentAs,
  uid,
} from "./repository";
import { parseRecordBody } from "./recordBodyFormat";
import { ViewportImage } from "./RecordBody";

export function RecordFiles({
  record,
  changed,
  onInsert,
  onBusy,
}: {
  record: RecordItem;
  changed: () => void;
  onInsert: (attachment: RecordAttachment) => void;
  onBusy: (busy: boolean) => void;
}) {
  const [progress, setProgress] = useState("");
  const [message, setMessage] = useState("");
  const [failed, setFailed] = useState<string[]>([]);
  const [preview, setPreview] = useState<string>();
  const [dragging, setDragging] = useState(false);
  const [dropError, setDropError] = useState("");
  const busy = useRef(false);
  const dropZone = useRef<HTMLElement>(null);
  const attachments = [
    ...new Map(
      (record.attachments || []).map((item) => [item.id, item]),
    ).values(),
  ];
  const references = parseRecordBody(
    record.renderedContent || record.notes || "",
  );

  async function archive(paths: string[]) {
    if (busy.current || !paths.length) return;
    busy.current = true;
    onBusy(true);
    setFailed([]);
    setMessage("");
    const errors: string[] = [];
    const retry: string[] = [];
    let imported = 0;
    try {
      for (const [index, path] of [...new Set(paths)].entries()) {
        const name = path.split(/[\\/]/).pop() || path;
        setProgress(`正在复制 ${index + 1}/${paths.length}：${name}`);
        try {
          await insertRecordFiles({
            recordId: record.id,
            files: [{ id: uid("attachment"), sourcePath: path }],
            archiveOnly: true,
            renderedContent: "",
            changeId: uid("record-change"),
            createdAt: new Date().toISOString(),
          });
          imported++;
        } catch (error) {
          errors.push(
            `${name}：${String(error instanceof Error ? error.message : error)}`,
          );
          retry.push(path);
        }
      }
      setMessage(
        [`已复制 ${imported} 个文件到 LabFlow。`, ...errors].join("\n"),
      );
      setFailed(retry);
      changed();
    } finally {
      setProgress("");
      busy.current = false;
      onBusy(false);
    }
  }
  const archiveRef = useRef(archive);
  useEffect(() => {
    archiveRef.current = archive;
  });
  useEffect(() => {
    if (!isTauri()) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void getCurrentWebview()
      .onDragDropEvent((event) => {
        const payload = event.payload;
        if (payload.type === "leave") {
          setDragging(false);
          return;
        }
        const rect = dropZone.current?.getBoundingClientRect();
        const x = payload.position.x / window.devicePixelRatio;
        const y = payload.position.y / window.devicePixelRatio;
        const inside =
          !!rect &&
          x >= rect.left &&
          x <= rect.right &&
          y >= rect.top &&
          y <= rect.bottom;
        setDragging(inside && payload.type !== "drop");
        if (payload.type === "drop" && inside)
          void archiveRef.current(payload.paths);
      })
      .then((off) => {
        if (disposed) off();
        else unlisten = off;
      })
      .catch(() => setDropError("拖放暂不可用，请点击“添加文件”。"));
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);
  async function action(run: () => Promise<unknown>) {
    try {
      await run();
    } catch (error) {
      setMessage(String(error instanceof Error ? error.message : error));
    }
  }
  return (
    <section
      id="record-files"
      className="record-section record-files"
      ref={dropZone}
      aria-label="实验文件"
    >
      <div className="section-title">
        <h2>
          实验文件 <small>({attachments.length})</small>
        </h2>
        <button
          className="primary"
          disabled={!!progress}
          onClick={() =>
            void action(async () => archive(await chooseRecordFiles()))
          }
        >
          添加文件
        </button>
      </div>
      <div className={`record-file-drop ${dragging ? "dragging" : ""}`}>
        {progress || "将文件拖到这里，或点击“添加文件”，可一次选择多个文件。"}
        <small>
          文件将复制到本次实验记录；添加文件不会修改正文。同名文件分别保留。
        </small>
      </div>
      {dropError && <p role="status">{dropError}</p>}
      {message && (
        <p className="record-file-message" role="status">
          {message}
        </p>
      )}
      {!!failed.length && (
        <button
          className="secondary"
          disabled={!!progress}
          onClick={() => void archive(failed)}
        >
          重试未导入的文件 ({failed.length})
        </button>
      )}
      {!attachments.length && (
        <p className="muted">
          还没有实验文件。可以添加原始数据、实验照片或分析结果。
        </p>
      )}
      <ul className="record-files-list">
        {attachments.map((attachment) => {
          const reference = references.find(
            (segment) =>
              segment.type !== "text" && segment.attachmentId === attachment.id,
          );
          const size =
            attachment.size === undefined
              ? ""
              : attachment.size < 1048576
                ? `${(attachment.size / 1024).toFixed(1)} KB`
                : `${(attachment.size / 1048576).toFixed(1)} MB`;
          return (
            <li key={attachment.id}>
              <div className="record-file-summary">
                <strong>{attachment.fileName}</strong>
                <small>
                  {[
                    attachment.fileName.split(".").pop()?.toUpperCase(),
                    size,
                    attachment.isAssayRaw
                      ? "检测数据导入"
                      : reference?.type === "image"
                        ? "正文插图"
                        : reference
                          ? "正文引用"
                          : "记录附件",
                  ]
                    .filter(Boolean)
                    .join(" · ")}
                </small>
              </div>
              <div className="record-file-actions">
                {attachment.previewRelativePath && (
                  <button
                    className="secondary"
                    onClick={() =>
                      setPreview(
                        preview === attachment.id ? undefined : attachment.id,
                      )
                    }
                  >
                    {preview === attachment.id ? "收起预览" : "预览"}
                  </button>
                )}
                <button
                  className="secondary"
                  onClick={() =>
                    void action(() =>
                      openRecordAttachment(record.id, attachment.id),
                    )
                  }
                >
                  打开
                </button>
                <button
                  className="secondary"
                  onClick={() =>
                    void action(() =>
                      saveRecordAttachmentAs(record.id, attachment),
                    )
                  }
                >
                  另存为
                </button>
                <button
                  className="secondary"
                  disabled={!!progress}
                  onClick={() => onInsert(attachment)}
                >
                  插入正文
                </button>
              </div>
              {preview === attachment.id && (
                <div className="record-file-preview">
                  <ViewportImage
                    attachment={attachment}
                    caption={attachment.fileName}
                  />
                </div>
              )}
            </li>
          );
        })}
      </ul>
    </section>
  );
}
