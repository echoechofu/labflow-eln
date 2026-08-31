import assert from "node:assert/strict";
import test from "node:test";
import { ResourceQueue } from "../src/resourceQueue.ts";
import {
  recordPdfBlocks,
  renderRecordPdf,
  type PdfBlock,
} from "../src/recordPdf.ts";
import { previewSize } from "../src/recordImageResources.ts";

test("resource permits cover lifetime, cancel queued requests, and release once", async () => {
  const queue = new ResourceQueue(1);
  const controller = new AbortController();
  const release = await queue.acquire(controller.signal);
  let granted = false;
  const cancelled = new AbortController();
  const rejected = assert.rejects(queue.acquire(cancelled.signal));
  const next = queue.acquire(controller.signal).then((permit) => {
    granted = true;
    return permit;
  });
  cancelled.abort();
  await rejected;
  assert.equal(granted, false);
  release();
  release();
  (await next)();
  (await queue.acquire(controller.signal))();
});

test("preview dimensions obey the 8 MiB pixel budget", () => {
  for (const [width, height] of [
    [2048, 2048],
    [5000, 20],
    [20, 5000],
    [1448, 1448],
  ]) {
    const preview = previewSize(width, height);
    assert.ok(preview.width * preview.height * 4 <= 8 * 1024 * 1024);
    assert.ok(preview.width <= width && preview.height <= height);
  }
});

test("100-image export retains one bitmap/page, handles cancellation and sink errors", async () => {
  let liveImages = 0;
  let peakImages = 0;
  let writes = 0;
  let peakWrites = 0;
  const canvases: { width: number; height: number }[] = [];
  const originals = ["document", "fetch", "createImageBitmap"].map(
    (key) => [key, Object.getOwnPropertyDescriptor(globalThis, key)] as const,
  );
  Object.defineProperty(globalThis, "document", {
    configurable: true,
    value: {
      fonts: { ready: Promise.resolve() },
      createElement: () => {
        const canvas = {
          width: 0,
          height: 0,
          getContext: () => ({
            fillRect() {},
            fillText() {},
            drawImage() {},
            measureText: (text: string) => ({ width: text.length * 10 }),
          }),
          toBlob: (callback: (blob: Blob) => void) =>
            callback(new Blob(["test jpeg"])),
        };
        canvases.push(canvas);
        return canvas;
      },
    },
  });
  Object.defineProperty(globalThis, "fetch", {
    configurable: true,
    value: async () => ({ ok: true, blob: async () => new Blob(["preview"]) }),
  });
  Object.defineProperty(globalThis, "createImageBitmap", {
    configurable: true,
    value: async () => {
      liveImages++;
      peakImages = Math.max(peakImages, liveImages);
      return {
        width: 1448,
        height: 1448,
        close: () => {
          liveImages--;
        },
      };
    },
  });
  const blocks: PdfBlock[] = Array.from({ length: 100 }, (_, i) => ({
    type: "image",
    id: String(i),
    caption: `图片 ${i}`,
  }));
  const options = {
    signal: new AbortController().signal,
    imageUrl: (id: string) => id,
    progress() {},
    writePage: async (_jpeg: Uint8Array, sequence: number) => {
      assert.equal(sequence, writes);
      peakWrites++;
      assert.equal(peakWrites, 1);
      await Promise.resolve();
      writes++;
      peakWrites--;
    },
  };
  try {
    const result = await renderRecordPdf(blocks, options);
    assert.equal(result.images, 100);
    assert.equal(result.pages, 100);
    assert.equal(peakImages, 1);
    assert.equal(liveImages, 0);
    assert.equal(canvases.length, 1);
    assert.equal(canvases[0].width, 0);
    const cancel = new AbortController();
    await assert.rejects(
      renderRecordPdf(blocks, {
        ...options,
        signal: cancel.signal,
        writePage: async () => {
          cancel.abort();
        },
      }),
    );
    assert.equal(liveImages, 0);
    await assert.rejects(
      renderRecordPdf(blocks, {
        ...options,
        writePage: async () => {
          throw new Error("disk full");
        },
      }),
      /disk full/,
    );
    assert.equal(liveImages, 0);
    assert.ok(
      canvases.every((canvas) => canvas.width === 0 && canvas.height === 0),
    );
  } finally {
    for (const [key, descriptor] of originals) {
      if (descriptor) Object.defineProperty(globalThis, key, descriptor);
      else Reflect.deleteProperty(globalThis, key);
    }
  }
});

test("missing inline attachments stop export instead of silently omitting evidence", () => {
  const record = {
    id: "r",
    taskId: "t",
    experimentId: "e",
    protocolId: "p",
    title: "实验",
    renderedContent: "![失踪图片](labflow-attachment://missing)",
    inputs: [],
    outputs: [],
    history: [],
    notes: "",
    updated: "",
  };
  const store = {
    records: [record],
    experiments: [],
    tasks: [],
    protocols: [],
    samples: [],
    sampleTypes: [],
  };
  assert.throws(
    () => [
      ...recordPdfBlocks([record], store, {
        id: "x",
        contentSha256: "hash",
        relativePath: "",
        recordCount: 1,
      }),
    ],
    /图片附件缺失/,
  );
});
