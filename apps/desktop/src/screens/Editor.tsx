import { getCurrentWebview } from "@tauri-apps/api/webview";
import { useEffect, useRef, useState } from "react";
import { Inspector } from "../editor/Inspector";
import { MediaLibrary } from "../editor/MediaLibrary";
import { PreviewPanel } from "../editor/PreviewPanel";
import { Timeline } from "../editor/Timeline";
import { SequenceTabs } from "../editor/SequenceTabs";
import { t } from "../i18n";
import { api } from "../ipc";
import { previewStore, run, uiStore } from "../store";
import type { SessionView } from "../types";

const MIN_TIMELINE_HEIGHT = 140;

export function Editor({ session }: { session: SessionView }) {
  const [timelineHeight, setTimelineHeight] = useState(300);
  const [dropping, setDropping] = useState(false);
  const editorRef = useRef<HTMLDivElement>(null);

  // OS file drops import into the media library (egui: `App::handle_dropped_files`).
  useEffect(() => {
    const unlisten = getCurrentWebview().onDragDropEvent((event) => {
      const payload = event.payload;
      if (payload.type === "enter" || payload.type === "over") {
        setDropping(true);
      } else if (payload.type === "drop") {
        setDropping(false);
        if (payload.paths.length > 0) void run(api.importMedia(payload.paths));
      } else {
        setDropping(false);
      }
    });
    return () => {
      void unlisten.then((stop) => stop());
    };
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.target instanceof HTMLInputElement || e.target instanceof HTMLSelectElement) return;
      const ctrl = e.ctrlKey || e.metaKey;
      const selected = uiStore.get().selectedClipIds;
      let handled = true;
      if (e.code === "Space") {
        void run(api.setPlaying(!previewStore.get().playing));
      } else if (ctrl && e.key.toLowerCase() === "z") {
        void run(e.shiftKey ? api.redo() : api.undo());
      } else if (ctrl && e.key.toLowerCase() === "y") {
        void run(api.redo());
      } else if (ctrl && e.key.toLowerCase() === "s") {
        void run(api.saveProject(e.shiftKey));
      } else if (ctrl && e.key.toLowerCase() === "i") {
        void run(api.importMediaDialog());
      } else if (!ctrl && e.key.toLowerCase() === "s") {
        void run(api.splitAtPlayhead(selected));
      } else if (!ctrl && e.key.toLowerCase() === "m") {
        void run(api.addMarker());
      } else if (e.key === "Delete" || e.key === "Backspace") {
        if (selected.length > 0) void run(api.deleteClips(selected, e.shiftKey));
      } else if (e.key === "ArrowLeft") {
        void run(api.step(e.shiftKey ? -10 : -1));
      } else if (e.key === "ArrowRight") {
        void run(api.step(e.shiftKey ? 10 : 1));
      } else if (e.key === "Home") {
        void run(api.seek(0));
      } else {
        handled = false;
      }
      if (handled) e.preventDefault();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const startResize = (e: React.PointerEvent<HTMLDivElement>) => {
    const startY = e.clientY;
    const startHeight = timelineHeight;
    const maxHeight = (editorRef.current?.clientHeight ?? 800) - 200;
    const target = e.currentTarget;
    target.setPointerCapture(e.pointerId);
    const move = (ev: PointerEvent) => {
      const next = startHeight + (startY - ev.clientY);
      setTimelineHeight(Math.max(MIN_TIMELINE_HEIGHT, Math.min(maxHeight, next)));
    };
    const up = () => {
      target.removeEventListener("pointermove", move);
      target.removeEventListener("pointerup", up);
    };
    target.addEventListener("pointermove", move);
    target.addEventListener("pointerup", up);
  };

  return (
    <div
      ref={editorRef}
      className="editor"
      style={{ "--timeline-height": `${timelineHeight}px` } as React.CSSProperties}
    >
      <MediaLibrary session={session} />
      <PreviewPanel />
      <Inspector session={session} />
      <div className="splitter" onPointerDown={startResize} />
      <SequenceTabs session={session} />
      <Timeline session={session} />
      {dropping && <div className="drop-overlay">{t("dropToImport")}</div>}
    </div>
  );
}
