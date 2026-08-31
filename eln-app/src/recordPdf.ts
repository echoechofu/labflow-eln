import type { RecordItem } from "./domain";
import type { Store, ExportManifestResult } from "./repository";
import { parseRecordBody } from "./recordBodyFormat";
import { previewSize, withRecordImage } from "./recordImageResources";

export type PdfBlock =
  | { type: "text"; text: string; size?: number; bold?: boolean }
  | { type: "image"; id: string; caption: string }
  | { type: "break" };

/** Metadata only: no image bytes, DOM nodes, or page canvases in this snapshot. */
export function* recordPdfBlocks(
  records: RecordItem[],
  store: Store,
  manifest: ExportManifestResult,
): Generator<PdfBlock> {
  const text = (text: string, size = 21, bold = false): PdfBlock => ({
    type: "text",
    text,
    size,
    bold,
  });
  const taskFor = (record: RecordItem) =>
    store.tasks.find((task) => task.id === record.taskId);
  yield text("LABFLOW ELECTRONIC LAB NOTEBOOK", 22, true);
  yield text("电子实验记录", 48, true);
  yield text(
    `日期范围：${taskFor(records[0])?.start.slice(0, 10) || ""} — ${taskFor(records.at(-1)!)?.start.slice(0, 10) || ""}`,
  );
  yield text(`记录数量：${records.length}`);
  yield text(`内容校验：${manifest.contentSha256}`, 18);
  yield text(
    "低内存图像 PDF · 文字不可选中复制；原始图片保留在 LabFlow 工作区。",
    18,
  );
  for (const record of records) {
    yield { type: "break" };
    const task = taskFor(record);
    const experiment = store.experiments.find(
      (item) => item.id === record.experimentId,
    );
    yield text(task?.start.replace("T", " ") || "", 20);
    yield text(record.title, 32, true);
    yield text(`${experiment?.code || ""} · ${experiment?.title || ""}`);
    yield text(
      `Protocol：${record.protocolName || record.protocolId} · v${record.protocolVersion || "snapshot"}`,
    );
    yield text(`Record ID：${record.id}`, 17);
    yield text("实验正文", 24, true);
    for (const segment of parseRecordBody(
      record.renderedContent || record.notes || "暂无正文。",
    )) {
      if (segment.type === "text") yield text(segment.text);
      else {
        const attachment = record.attachments?.find(
          (item) => item.id === segment.attachmentId,
        );
        if (!attachment)
          throw new Error(
            `图片附件缺失：${segment.caption || segment.attachmentId}`,
          );
        yield {
          type: "image",
          id: attachment.id,
          caption: segment.caption || attachment.fileName,
        };
      }
    }
    for (const section of record.analysisSections || []) {
      yield text(section.title, 24, true);
      yield text(section.text);
    }
    yield text("样本", 24, true);
    const samples = (ids: string[]) =>
      ids
        .map((id) => store.samples.find((item) => item.id === id)?.code || id)
        .join("、") || "无";
    yield text(`输入：${samples(record.inputs)}`);
    yield text(`输出：${samples(record.outputs)}`);
    if (record.results?.length) yield text("Results", 24, true);
    for (const result of record.results || [])
      yield text(`${result.type} · ${JSON.stringify(result.data)}`);
    if (record.attachments?.length) yield text("附件目录", 24, true);
    for (const attachment of record.attachments || [])
      yield text(`${attachment.fileName} · ${attachment.relativePath}`, 18);
  }
}

export const PDF_PAGE_WIDTH = 1240;
export const PDF_PAGE_HEIGHT = 1754;
const MARGIN = 100;
const BODY_WIDTH = PDF_PAGE_WIDTH - MARGIN * 2;
const BOTTOM = PDF_PAGE_HEIGHT - MARGIN;

/** O(one page + one image): the sink must await durable page consumption. */
export async function renderRecordPdf(
  blocks: Iterable<PdfBlock>,
  options: {
    signal: AbortSignal;
    imageUrl: (id: string) => string;
    writePage: (jpeg: Uint8Array, sequence: number) => Promise<void>;
    progress: (pages: number, images: number) => void;
  },
) {
  const { signal } = options;
  const canvas = document.createElement("canvas");
  canvas.width = PDF_PAGE_WIDTH;
  canvas.height = PDF_PAGE_HEIGHT;
  const context = canvas.getContext("2d", { alpha: false });
  if (!context) throw new Error("无法创建 PDF 页画布");
  let y = MARGIN;
  let pages = 0;
  let images = 0;
  const reset = () => {
    context.fillStyle = "white";
    context.fillRect(0, 0, canvas.width, canvas.height);
    context.fillStyle = "#252337";
    y = MARGIN;
  };
  const flush = async () => {
    signal.throwIfAborted();
    if (y === MARGIN) return;
    context.fillStyle = "#777489";
    context.font = "16px sans-serif";
    context.fillText(`LabFlow · ${pages + 1}`, MARGIN, PDF_PAGE_HEIGHT - 48);
    const blob = await new Promise<Blob>((resolve, reject) => {
      canvas.toBlob(
        (blob) => (blob ? resolve(blob) : reject(new Error("PDF 页编码失败"))),
        "image/jpeg",
        0.94,
      );
    });
    signal.throwIfAborted();
    await options.writePage(new Uint8Array(await blob.arrayBuffer()), pages);
    pages++;
    options.progress(pages, images);
    reset();
    // Yield between pages so cancel/paint events are serviced.
    await new Promise<void>((resolve) => setTimeout(resolve, 0));
  };
  const paragraph = async (text: string, size = 21, bold = false) => {
    const lineHeight = Math.ceil(size * 1.65);
    // Keep a heading with at least the beginning of its following content.
    if (bold && y + lineHeight * 3 > BOTTOM) await flush();
    const setStyle = () => {
      context.font = `${bold ? "600" : "400"} ${size}px -apple-system, BlinkMacSystemFont, "Segoe UI", "Microsoft YaHei", sans-serif`;
      context.fillStyle = "#252337";
    };
    const line = async (value: string) => {
      signal.throwIfAborted();
      if (y + lineHeight > BOTTOM) await flush();
      setStyle();
      context.fillText(value, MARGIN, y + size);
      y += lineHeight;
    };
    setStyle();
    // Grapheme segmentation preserves combining marks and emoji; no giant
    // all-pages layout array or all-characters spread is retained.
    const segmenter = new Intl.Segmenter(undefined, {
      granularity: "grapheme",
    });
    for (const raw of text.replaceAll("\r\n", "\n").split("\n")) {
      let value = "";
      for (const { segment } of segmenter.segment(
        raw.replaceAll("\t", "    "),
      )) {
        if (value && context.measureText(value + segment).width > BODY_WIDTH) {
          await line(value);
          value = "";
        }
        value += segment;
      }
      await line(value);
    }
    y += 10;
  };
  reset();
  try {
    await document.fonts.ready;
    for (const block of blocks) {
      signal.throwIfAborted();
      if (block.type === "break") await flush();
      else if (block.type === "text")
        await paragraph(block.text, block.size, block.bold);
      else {
        await withRecordImage(
          options.imageUrl(block.id),
          signal,
          async (image) => {
            const size = previewSize(image.width, image.height);
            const scale = Math.min(
              1,
              BODY_WIDTH / size.width,
              (BOTTOM - MARGIN - 80) / size.height,
            );
            const width = size.width * scale;
            const height = size.height * scale;
            if (y + height + 55 > BOTTOM) await flush();
            context.drawImage(
              image,
              MARGIN + (BODY_WIDTH - width) / 2,
              y,
              width,
              height,
            );
            y += height + 12;
          },
        );
        images++;
        await paragraph(block.caption, 17);
        options.progress(pages, images);
      }
    }
    await flush();
    return { pages, images };
  } finally {
    // Explicitly release canvas backing storage, including cancellation/error.
    canvas.width = 0;
    canvas.height = 0;
  }
}
