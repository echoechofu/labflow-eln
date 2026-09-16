export type RecordBodySegment =
  | { type: "text"; text: string }
  | { type: "image"; attachmentId: string; caption: string }
  | { type: "file"; attachmentId: string; label: string };

const ATTACHMENT_REFERENCE =
  /(!?)\[([^\]\r\n]*)\]\(labflow-(attachment|file):\/\/([A-Za-z0-9-]+)\)/g;

const safeCaption = (value: string) =>
  value
    .replaceAll("[", "(")
    .replaceAll("]", ")")
    .replace(/[\r\n]+/g, " ")
    .trim();

export function parseRecordBody(content: string): RecordBodySegment[] {
  const segments: RecordBodySegment[] = [];
  let cursor = 0;
  for (const match of content.matchAll(ATTACHMENT_REFERENCE)) {
    const index = match.index ?? 0;
    if (index > cursor) {
      segments.push({ type: "text", text: content.slice(cursor, index) });
    }
    const isImage = match[1] === "!" && match[3] === "attachment";
    segments.push(
      isImage
        ? { type: "image", caption: match[2], attachmentId: match[4] }
        : { type: "file", label: match[2], attachmentId: match[4] },
    );
    cursor = index + match[0].length;
  }
  if (cursor < content.length || segments.length === 0) {
    segments.push({ type: "text", text: content.slice(cursor) });
  }
  return segments;
}

export function attachmentLabelFromPath(path: string) {
  return safeCaption(path.split(/[\\/]/).at(-1) || "附件") || "附件";
}

export function imageCaptionFromPath(path: string) {
  const fileName = path.split(/[\\/]/).at(-1) || "实验图片";
  return safeCaption(fileName.replace(/\.[^.]+$/, "")) || "实验图片";
}

export function insertImageReference(
  content: string,
  offset: number,
  attachmentId: string,
  caption: string,
) {
  const safeOffset = Math.max(0, Math.min(offset, content.length));
  const before = content.slice(0, safeOffset);
  const after = content.slice(safeOffset);
  const marker = `![${safeCaption(caption)}](labflow-attachment://${attachmentId})`;
  const prefix = before && !before.endsWith("\n") ? "\n\n" : "";
  const suffix = after && !after.startsWith("\n") ? "\n\n" : "";
  const inserted = `${prefix}${marker}${suffix}`;
  return {
    content: `${before}${inserted}${after}`,
    cursor: before.length + inserted.length,
  };
}

export function insertFileReferences(
  content: string,
  offset: number,
  attachments: { id: string; label: string }[],
) {
  const safeOffset = Math.max(0, Math.min(offset, content.length));
  const before = content.slice(0, safeOffset);
  const after = content.slice(safeOffset);
  const markers = attachments
    .map(
      ({ id, label }) =>
        `[附件：${safeCaption(label)}](labflow-file://${id})`,
    )
    .join("\n");
  const prefix = before && !before.endsWith("\n") ? "\n\n" : "";
  const suffix = after && !after.startsWith("\n") ? "\n\n" : "";
  const inserted = `${prefix}${markers}${suffix}`;
  return {
    content: `${before}${inserted}${after}`,
    cursor: before.length + inserted.length,
  };
}
