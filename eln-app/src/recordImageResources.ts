import { ResourceQueue } from "./resourceQueue";

// No decoded-image cache: keep at most 4 visible preview canvases (32 MiB),
// and serialize bitmap decoding for both preview and export.
export const visibleImageSlots = new ResourceQueue(4);
const decodeSlots = new ResourceQueue(1);
export const IMAGE_MAX_EDGE = 1448;
const MAX_ENCODED_BYTES = 16 * 1024 * 1024;

export async function withRecordImage<T>(
  url: string,
  signal: AbortSignal,
  consume: (image: ImageBitmap) => Promise<T> | T,
): Promise<T> {
  const release = await decodeSlots.acquire(signal);
  let bitmap: ImageBitmap | undefined;
  try {
    signal.throwIfAborted();
    const response = await fetch(url, { signal, cache: "no-store" });
    if (!response.ok) throw new Error("图片读取失败，请检查附件是否存在");
    const blob = await response.blob();
    if (blob.size > MAX_ENCODED_BYTES)
      throw new Error("预览文件超过 16 MiB，请重新插入图片生成预览");
    signal.throwIfAborted();
    // Backend validates dimensions before sending bytes. This also supports
    // legacy 2048px previews without modifying any saved attachment.
    bitmap = await createImageBitmap(blob);
    signal.throwIfAborted();
    if (bitmap.width > 2048 || bitmap.height > 2048) {
      throw new Error("旧图片缺少受限预览，请重新插入图片");
    }
    return await consume(bitmap);
  } finally {
    bitmap?.close();
    release();
  }
}

export function previewSize(width: number, height: number) {
  const scale = Math.min(1, IMAGE_MAX_EDGE / width, IMAGE_MAX_EDGE / height);
  return {
    width: Math.max(1, Math.floor(width * scale)),
    height: Math.max(1, Math.floor(height * scale)),
  };
}
