import assert from "node:assert/strict";
import test from "node:test";
import {
  attachmentLabelFromPath,
  imageCaptionFromPath,
  insertFileReferences,
  insertImageReference,
  parseRecordBody,
} from "../src/recordBodyFormat";

test("record body parser preserves text around inline attachment references", () => {
  const segments = parseRecordBody(
    "before\n![Day 3](labflow-attachment://attachment-1)\nafter",
  );
  assert.deepEqual(segments, [
    { type: "text", text: "before\n" },
    { type: "image", caption: "Day 3", attachmentId: "attachment-1" },
    { type: "text", text: "\nafter" },
  ]);
});

test("file attachments are parsed separately from previewable images", () => {
  const inserted = insertFileReferences("result", 6, [
    { id: "attachment-csv", label: "raw data.csv" },
    { id: "attachment-pdf", label: "instrument.pdf" },
  ]);
  assert.equal(
    inserted.content,
    "result\n\n[附件：raw data.csv](labflow-file://attachment-csv)\n[附件：instrument.pdf](labflow-file://attachment-pdf)",
  );
  assert.deepEqual(parseRecordBody(inserted.content).slice(1), [
    { type: "file", label: "附件：raw data.csv", attachmentId: "attachment-csv" },
    { type: "text", text: "\n" },
    { type: "file", label: "附件：instrument.pdf", attachmentId: "attachment-pdf" },
  ]);
  assert.equal(attachmentLabelFromPath("C:\\Data\\result.xlsx"), "result.xlsx");
});

test("image insertion creates a stable reference at the cursor", () => {
  const inserted = insertImageReference("before after", 6, "attachment-1", "Image");
  assert.equal(
    inserted.content,
    "before\n\n![Image](labflow-attachment://attachment-1)\n\n after",
  );
});

test("caption derives from either macOS or Windows paths", () => {
  assert.equal(imageCaptionFromPath("/tmp/Day 3.png"), "Day 3");
  assert.equal(imageCaptionFromPath("C:\\Data\\WB.tiff"), "WB");
});
