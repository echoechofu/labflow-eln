import assert from "node:assert/strict";
import test from "node:test";
import {
  imageCaptionFromPath,
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
