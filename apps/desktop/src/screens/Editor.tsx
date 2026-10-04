import { getCurrentWebview } from "@tauri-apps/api/webview";
import { useEffect, useRef, useState } from "react";
import { Inspector } from "../editor/Inspector";
import { MediaLibrary } from "../editor/MediaLibrary";
import { PreviewPanel } from "../editor/PreviewPanel";
import { Timeline } from "../editor/Timeline";
import { SequenceTabs } from "../editor/SequenceTabs";
import { t } from "../i18n";
import { api } from "../ipc";
import { patchUi, previewStore, run, uiStore } from "../store";
import type { SessionView } from "../types";

const MIN_TIMELINE_HEIGHT = 140;
const MIN_LIBRARY_WIDTH = 160;
const MIN_INSPECTOR_WIDTH = 180;
const MAX_PANEL_WIDTH = 640;

function initialLayout(session: SessionView) {
  const layout = session.project.panel_layout;
  return {
    libraryWidth: Math.max(MIN_LIBRARY_WIDTH, Math.min(MAX_PANEL_WIDTH, layout?.lib_panel_width ?? 260)),
    inspectorWidth: Math.max(MIN_INSPECTOR_WIDTH, Math.min(MAX_PANEL_WIDTH, layout?.props_panel_width ?? 280)),
    timelineHeight: Math.max(MIN_TIMELINE_HEIGHT, layout?.timeline_height ?? 300),
  };
}

export function Editor({ session }: { session: SessionView }) {
  const [layout, setLayout] = useState(() => initialLayout(session));
  const [dropping, setDropping] = useState(false);
  const editorRef = useRef<HTMLDivElement>(null);

  useEffect(() => setLayout(initialLayout(session)), [session.project.id]);

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
      const drawingPoints = uiStore.get().drawingShapePoints;
      if (drawingPoints !== null) {
        // Swallows every key while drawing a custom shape -- only Enter (finish, >=3 points,
        // ui::App::finish_drawing_custom_shape's own floor) and Escape (cancel) apply, matching
        // egui's own drawing-mode key handling.
        if (e.key === "Enter" && drawingPoints.length >= 3) {
          void run(api.addCustomShapeClip(drawingPoints)).then((id) => {
            patchUi({ drawingShapePoints: null });
            if (id != null) patchUi({ selectedClipIds: [id] });
          });
        } else if (e.key === "Escape") {
          patchUi({ drawingShapePoints: null });
        }
        e.preventDefault();
        return;
      }
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
      } else if (ctrl && e.shiftKey && e.key.toLowerCase() === "c") {
        if (selected[0] !== undefined) void run(api.copyClipFormatting(selected[0]));
      } else if (ctrl && e.shiftKey && e.key.toLowerCase() === "v") {
        if (selected[0] !== undefined) void run(api.pasteClipFormatting(selected[0]));
      } else if (ctrl && e.key.toLowerCase() === "c") {
        if (selected[0] !== undefined) void run(api.copyClip(selected[0]));
      } else if (ctrl && e.key.toLowerCase() === "x") {
        if (selected[0] !== undefined) void run(api.cutClip(selected[0]));
      } else if (ctrl && e.key.toLowerCase() === "v") {
        void run(api.pasteClip()).then((ids) => {
          if (ids && ids.length > 0) patchUi({ selectedClipIds: ids });
        });
      } else if (!ctrl && e.key.toLowerCase() === "s") {
        void run(api.splitAtPlayhead(selected));
      } else if (!ctrl && e.key.toLowerCase() === "m") {
        void run(api.addMarker());
      } else if (!ctrl && e.key.toLowerCase() === "t") {
        patchUi({ tool: "text" });
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
    const startHeight = layout.timelineHeight;
    const maxHeight = (editorRef.current?.clientHeight ?? 800) - 200;
    const target = e.currentTarget;
    target.setPointerCapture(e.pointerId);
    const move = (ev: PointerEvent) => {
      const next = startHeight + (startY - ev.clientY);
      setLayout((current) => ({ ...current, timelineHeight: Math.max(MIN_TIMELINE_HEIGHT, Math.min(maxHeight, next)) }));
    };
    const up = () => {
      target.removeEventListener("pointermove", move);
      target.removeEventListener("pointerup", up);
      const current = uiLayout.current;
      void run(api.setPanelLayout(current.libraryWidth, current.inspectorWidth, current.timelineHeight));
    };
    target.addEventListener("pointermove", move);
    target.addEventListener("pointerup", up);
  };

  const uiLayout = useRef(layout);
  uiLayout.current = layout;
  const startPanelResize = (side: "library" | "inspector") => (e: React.PointerEvent<HTMLDivElement>) => {
    const startX = e.clientX;
    const initial = layout;
    const target = e.currentTarget;
    target.setPointerCapture(e.pointerId);
    const move = (event: PointerEvent) => {
      const delta = event.clientX - startX;
      setLayout((current) => ({
        ...current,
        libraryWidth: side === "library" ? Math.max(MIN_LIBRARY_WIDTH, Math.min(MAX_PANEL_WIDTH, initial.libraryWidth + delta)) : current.libraryWidth,
        inspectorWidth: side === "inspector" ? Math.max(MIN_INSPECTOR_WIDTH, Math.min(MAX_PANEL_WIDTH, initial.inspectorWidth - delta)) : current.inspectorWidth,
      }));
    };
    const up = () => {
      target.removeEventListener("pointermove", move);
      target.removeEventListener("pointerup", up);
      const current = uiLayout.current;
      void run(api.setPanelLayout(current.libraryWidth, current.inspectorWidth, current.timelineHeight));
    };
    target.addEventListener("pointermove", move);
    target.addEventListener("pointerup", up);
  };

  return (
    <div
      ref={editorRef}
      className="editor"
      data-testid="screen-editor"
      style={{
        "--library-width": `${layout.libraryWidth}px`,
        "--inspector-width": `${layout.inspectorWidth}px`,
        "--timeline-height": `${layout.timelineHeight}px`,
      } as React.CSSProperties}
    >
      <MediaLibrary session={session} />
      <div className="panel-splitter library-splitter" onPointerDown={startPanelResize("library")} />
      <PreviewPanel />
      <div className="panel-splitter inspector-splitter" onPointerDown={startPanelResize("inspector")} />
      <Inspector session={session} />
      <div className="splitter" onPointerDown={startResize} />
      <SequenceTabs session={session} />
      <Timeline session={session} />
      {dropping && <div className="drop-overlay">{t("dropToImport")}</div>}
    </div>
  );
}
