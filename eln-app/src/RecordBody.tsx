import type { RecordAttachment } from "./domain";
import { recordImagePreviewUrl } from "./repository";
import { parseRecordBody } from "./recordBodyFormat";
import { useEffect, useRef, useState } from "react";
import {
  previewSize,
  visibleImageSlots,
  withRecordImage,
} from "./recordImageResources";

function ViewportImage({
  attachment,
  caption,
}: {
  attachment: RecordAttachment;
  caption: string;
}) {
  const container = useRef<HTMLDivElement>(null);
  const canvas = useRef<HTMLCanvasElement>(null);
  const [visible, setVisible] = useState(false);
  const [error, setError] = useState("");
  const [loaded, setLoaded] = useState(false);
  useEffect(() => {
    const observer = new IntersectionObserver(([entry]) => {
      setVisible(entry.isIntersecting);
      if (!entry.isIntersecting) setLoaded(false);
    });
    if (container.current) observer.observe(container.current);
    return () => observer.disconnect();
  }, []);
  useEffect(() => {
    if (!visible) return;
    const controller = new AbortController();
    const target = canvas.current;
    let releaseSlot: (() => void) | undefined;
    void (async () => {
      releaseSlot = await visibleImageSlots.acquire(controller.signal);
      controller.signal.throwIfAborted();
      setError("");
      setLoaded(false);
      await withRecordImage(
        recordImagePreviewUrl(attachment.id),
        controller.signal,
        (image) => {
          if (!target) return;
          const size = previewSize(image.width, image.height);
          target.width = size.width;
          target.height = size.height;
          const context = target.getContext("2d");
          if (!context) throw new Error("无法显示图片");
          context.drawImage(image, 0, 0, size.width, size.height);
          setLoaded(true);
        },
      );
    })().catch((reason) => {
      releaseSlot?.();
      if (!controller.signal.aborted) setError(String(reason));
    });
    return () => {
      controller.abort();
      releaseSlot?.();
      if (target) {
        target.width = 0;
        target.height = 0;
      }
    };
  }, [visible, attachment.id]);
  const ratio =
    attachment.widthPx && attachment.heightPx
      ? attachment.widthPx / attachment.heightPx
      : 4 / 3;
  return (
    <div
      ref={container}
      className="record-image-viewport"
      style={{ aspectRatio: ratio }}
    >
      <canvas
        ref={canvas}
        width={0}
        height={0}
        role="img"
        aria-label={caption}
      />
      {(!visible || !loaded) && (
        <span
          className={
            error ? "record-image-missing" : "record-image-placeholder"
          }
        >
          {error || "图片按需加载"}
        </span>
      )}
    </div>
  );
}

export function RecordBody({
  content,
  attachments = [],
  eager = false,
  className = "",
}: {
  content: string;
  attachments?: RecordAttachment[];
  eager?: boolean;
  className?: string;
}) {
  const attachmentMap = new Map(attachments.map((item) => [item.id, item]));
  return (
    <div className={`record-rich-body ${className}`.trim()}>
      {parseRecordBody(content).map((segment, index) => {
        if (segment.type === "text") {
          return segment.text ? (
            <span className="record-body-text" key={`text-${index}`}>
              {segment.text}
            </span>
          ) : null;
        }
        const attachment = attachmentMap.get(segment.attachmentId);
        if (!attachment) {
          return (
            <p
              className="record-image-missing"
              key={`image-${segment.attachmentId}-${index}`}
            >
              图片附件缺失：{segment.caption || segment.attachmentId}
            </p>
          );
        }
        return (
          <figure
            className="record-image"
            key={`image-${attachment.id}-${index}`}
          >
            {eager ? (
              <img
                alt={segment.caption || attachment.fileName}
                data-record-image
                decoding="async"
                loading={eager ? "eager" : "lazy"}
                src={recordImagePreviewUrl(attachment.id)}
              />
            ) : (
              <ViewportImage
                attachment={attachment}
                caption={segment.caption || attachment.fileName}
              />
            )}
            <figcaption>
              {segment.caption || attachment.fileName}
              {attachment.widthPx && attachment.heightPx
                ? ` · ${attachment.widthPx} × ${attachment.heightPx}px`
                : ""}
            </figcaption>
          </figure>
        );
      })}
    </div>
  );
}
