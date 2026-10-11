/** Pixels the pointer must travel before a press becomes a drag. */
const DRAG_THRESHOLD_PX = 4;

/**
 * Follows the pointer after a press on a library item. Drags between elements use pointer events:
 * with Tauri's OS file drop enabled, WebView2 does not deliver HTML5 drag-and-drop between them.
 *
 * `onDrag` runs on every move once the pointer has travelled past the threshold. `onClear` runs
 * when the drag ends or is cancelled (Escape, the window losing focus, the system taking the
 * pointer); on a normal release it runs in a macrotask, because the drop target's own `pointerup`
 * listener runs after this one and must still see the drag. A cancelled drag clears at once so
 * it cannot drop anywhere. Returns whether the press turned into a drag, for the click that may
 * follow it.
 */
export function trackPointerDrag(
  press: { clientX: number; clientY: number },
  onDrag: (event: PointerEvent) => void,
  onClear: () => void,
): { wasDrag: () => boolean } {
  let dragging = false;
  const move = (ev: PointerEvent) => {
    if (
      !dragging &&
      Math.hypot(ev.clientX - press.clientX, ev.clientY - press.clientY) < DRAG_THRESHOLD_PX
    ) {
      return;
    }
    dragging = true;
    onDrag(ev);
  };
  const stop = () => {
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", end);
    window.removeEventListener("pointercancel", cancel);
    window.removeEventListener("blur", cancel);
    window.removeEventListener("keydown", onKey);
  };
  const end = () => {
    stop();
    setTimeout(onClear, 0);
  };
  const cancel = () => {
    onClear();
    stop();
  };
  const onKey = (ev: KeyboardEvent) => {
    if (ev.key === "Escape") {
      cancel();
    }
  };
  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", end);
  window.addEventListener("pointercancel", cancel);
  window.addEventListener("blur", cancel);
  window.addEventListener("keydown", onKey);
  return {
    wasDrag: () => {
      return dragging;
    },
  };
}
