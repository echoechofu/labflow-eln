import { useRef, type PointerEvent } from "react";

// Pointer capture supports dragging within desktop webviews without native file-drop handling.
export function useComposerDrag(
  drop: (value: string, target: Element) => void,
) {
  const active = useRef<{
    x: number;
    y: number;
    value: string;
    moved: boolean;
  } | null>(null);
  const suppressClick = useRef(false);
  return (value: string, click?: () => void) => ({
    onPointerDown(event: PointerEvent<HTMLButtonElement>) {
      if (event.button !== 0) return;
      active.current = {
        x: event.clientX,
        y: event.clientY,
        value,
        moved: false,
      };
      suppressClick.current = false;
      event.currentTarget.setPointerCapture(event.pointerId);
    },
    onPointerMove(event: PointerEvent<HTMLButtonElement>) {
      const drag = active.current;
      if (
        drag &&
        Math.hypot(event.clientX - drag.x, event.clientY - drag.y) > 6
      ) {
        drag.moved = true;
        event.currentTarget.style.opacity = "0.5";
      }
    },
    onPointerUp(event: PointerEvent<HTMLButtonElement>) {
      const drag = active.current;
      active.current = null;
      event.currentTarget.style.opacity = "";
      suppressClick.current = Boolean(drag?.moved);
      if (drag?.moved) {
        const target = document.elementFromPoint(event.clientX, event.clientY);
        if (target) drop(drag.value, target);
      }
    },
    onPointerCancel(event: PointerEvent<HTMLButtonElement>) {
      active.current = null;
      event.currentTarget.style.opacity = "";
    },
    onClick() {
      if (!suppressClick.current) click?.();
      suppressClick.current = false;
    },
  });
}
