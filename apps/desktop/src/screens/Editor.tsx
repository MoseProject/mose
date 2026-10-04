import { getCurrentWebview } from "@tauri-apps/api/webview";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { AudioMeter } from "../editor/AudioMeter";
import { ToolRail } from "../editor/ToolRail";
import { Inspector } from "../editor/Inspector";
import { MediaLibrary } from "../editor/MediaLibrary";
import { PreviewPanel } from "../editor/PreviewPanel";
import { Timeline } from "../editor/Timeline";
import { t } from "../i18n";
import { api } from "../ipc";
import { isActivatableTarget, shouldIgnoreShortcut } from "../keyboard";
import { patchUi, previewStore, run, uiStore } from "../store";
import type { SessionView } from "../types";

const MIN_TIMELINE_HEIGHT = 140;
const MIN_LIBRARY_WIDTH = 160;
const MIN_INSPECTOR_WIDTH = 180;
const MAX_PANEL_WIDTH = 640;

function initialLayout(session: SessionView) {
  const layout = session.project.panel_layout;
  return {
    libraryWidth: Math.max(
      MIN_LIBRARY_WIDTH,
      Math.min(MAX_PANEL_WIDTH, layout?.lib_panel_width ?? 340),
    ),
    inspectorWidth: Math.max(
      MIN_INSPECTOR_WIDTH,
      Math.min(MAX_PANEL_WIDTH, layout?.props_panel_width ?? 296),
    ),
    timelineHeight: Math.max(MIN_TIMELINE_HEIGHT, layout?.timeline_height ?? 392),
  };
}

export function Editor({ session }: { session: SessionView }) {
  const [layoutState, setLayoutState] = useState(() => {
    return { projectId: session.project.id, layout: initialLayout(session) };
  });
  // A different project brings its own stored layout; adjust during render, not in an effect.
  let layout = layoutState.layout;
  if (layoutState.projectId !== session.project.id) {
    layout = initialLayout(session);
    setLayoutState({ projectId: session.project.id, layout });
  }
  const setLayout = (update: (current: typeof layout) => typeof layout) => {
    setLayoutState((state) => {
      return { projectId: state.projectId, layout: update(state.layout) };
    });
  };
  const uiLayout = useRef(layout);
  useLayoutEffect(() => {
    uiLayout.current = layout;
  });
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
        if (payload.paths.length > 0) {
          void run(api.importMedia(payload.paths));
        }
      } else {
        setDropping(false);
      }
    });
    return () => {
      void unlisten.then((stop) => {
        return stop();
      });
    };
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (shouldIgnoreShortcut(e, uiStore.get().openDialogs)) {
        return;
      }
      const ctrl = e.ctrlKey || e.metaKey;
      const drawingPoints = uiStore.get().drawingShapePoints;
      if (drawingPoints !== null) {
        // Swallows every key while drawing a custom shape -- only Enter (finish, >=3 points,
        // ui::App::finish_drawing_custom_shape's own floor) and Escape (cancel) apply, matching
        // egui's own drawing-mode key handling.
        if (e.key === "Enter" && drawingPoints.length >= 3) {
          void run(api.addCustomShapeClip(drawingPoints)).then((id) => {
            patchUi({ drawingShapePoints: null });
            if (id != null) {
              patchUi({ selectedClipIds: [id] });
            }
          });
        } else if (e.key === "Escape") {
          patchUi({ drawingShapePoints: null });
        }
        e.preventDefault();
        return;
      }
      const selected = uiStore.get().selectedClipIds;
      let handled = true;
      if (e.code === "Space" && isActivatableTarget(e.target)) {
        return;
      }
      if (e.code === "Space") {
        void run(api.setPlaying(!previewStore.get().playing));
      } else if (ctrl && e.key.toLowerCase() === "z") {
        void run(e.shiftKey ? api.redo() : api.undo());
      } else if (ctrl && e.key.toLowerCase() === "y") {
        void run(api.redo());
      } else if (ctrl && e.key.toLowerCase() === "s") {
        void run(api.saveProject(e.shiftKey));
      } else if (ctrl && e.key.toLowerCase() === "e") {
        patchUi({ exportDialogOpen: true });
      } else if (ctrl && e.key.toLowerCase() === "i") {
        void run(api.importMediaDialog());
      } else if (ctrl && e.shiftKey && e.key.toLowerCase() === "c") {
        if (selected[0] !== undefined) {
          void run(api.copyClipFormatting(selected[0]));
        }
      } else if (ctrl && e.shiftKey && e.key.toLowerCase() === "v") {
        if (selected[0] !== undefined) {
          void run(api.pasteClipFormatting(selected[0]));
        }
      } else if (ctrl && e.key.toLowerCase() === "c") {
        if (selected[0] !== undefined) {
          void run(api.copyClip(selected[0]));
        }
      } else if (ctrl && e.key.toLowerCase() === "x") {
        if (selected[0] !== undefined) {
          void run(api.cutClip(selected[0]));
        }
      } else if (ctrl && e.key.toLowerCase() === "v") {
        void run(api.pasteClip()).then((ids) => {
          if (ids && ids.length > 0) {
            patchUi({ selectedClipIds: ids });
          }
        });
      } else if (!ctrl && e.key.toLowerCase() === "s") {
        void run(api.splitAtPlayhead(selected));
      } else if (!ctrl && e.key.toLowerCase() === "m") {
        void run(api.addMarker());
      } else if (!ctrl && e.key.toLowerCase() === "t") {
        patchUi({ tool: "text" });
      } else if (e.key === "Delete" || e.key === "Backspace") {
        if (selected.length > 0) {
          void run(api.deleteClips(selected, e.shiftKey));
        }
      } else if (e.key === "ArrowLeft") {
        void run(api.step(e.shiftKey ? -10 : -1));
      } else if (e.key === "ArrowRight") {
        void run(api.step(e.shiftKey ? 10 : 1));
      } else if (e.key === "Home") {
        void run(api.seek(0));
      } else {
        handled = false;
      }
      if (handled) {
        e.preventDefault();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => {
      return window.removeEventListener("keydown", onKey);
    };
  }, []);

  const startResize = (e: React.PointerEvent<HTMLDivElement>) => {
    const startY = e.clientY;
    const startHeight = layout.timelineHeight;
    const maxHeight = (editorRef.current?.clientHeight ?? 800) - 200;
    const target = e.currentTarget;
    target.setPointerCapture(e.pointerId);
    const move = (ev: PointerEvent) => {
      const next = startHeight + (startY - ev.clientY);
      setLayout((current) => {
        return {
          ...current,
          timelineHeight: Math.max(MIN_TIMELINE_HEIGHT, Math.min(maxHeight, next)),
        };
      });
    };
    const up = () => {
      target.removeEventListener("pointermove", move);
      target.removeEventListener("pointerup", up);
      const current = uiLayout.current;
      void run(
        api.setPanelLayout(current.libraryWidth, current.inspectorWidth, current.timelineHeight),
      );
    };
    target.addEventListener("pointermove", move);
    target.addEventListener("pointerup", up);
  };

  const startPanelResize = (side: "library" | "inspector") => {
    return (e: React.PointerEvent<HTMLDivElement>) => {
      const startX = e.clientX;
      const initial = layout;
      const target = e.currentTarget;
      target.setPointerCapture(e.pointerId);
      const move = (event: PointerEvent) => {
        const delta = event.clientX - startX;
        setLayout((current) => {
          return {
            ...current,
            libraryWidth:
              side === "library"
                ? Math.max(
                    MIN_LIBRARY_WIDTH,
                    Math.min(MAX_PANEL_WIDTH, initial.libraryWidth + delta),
                  )
                : current.libraryWidth,
            inspectorWidth:
              side === "inspector"
                ? Math.max(
                    MIN_INSPECTOR_WIDTH,
                    Math.min(MAX_PANEL_WIDTH, initial.inspectorWidth - delta),
                  )
                : current.inspectorWidth,
          };
        });
      };
      const up = () => {
        target.removeEventListener("pointermove", move);
        target.removeEventListener("pointerup", up);
        const current = uiLayout.current;
        void run(
          api.setPanelLayout(current.libraryWidth, current.inspectorWidth, current.timelineHeight),
        );
      };
      target.addEventListener("pointermove", move);
      target.addEventListener("pointerup", up);
    };
  };

  return (
    <div
      ref={editorRef}
      className="editor"
      data-testid="screen-editor"
      style={
        {
          "--library-width": `${layout.libraryWidth}px`,
          "--inspector-width": `${layout.inspectorWidth}px`,
          "--timeline-height": `${layout.timelineHeight}px`,
        } as React.CSSProperties
      }
    >
      <ToolRail />
      <MediaLibrary session={session} />
      <div
        className="panel-splitter library-splitter"
        onPointerDown={(e) => {
          startPanelResize("library")(e);
        }}
      />
      <PreviewPanel />
      <div
        className="panel-splitter inspector-splitter"
        onPointerDown={(e) => {
          startPanelResize("inspector")(e);
        }}
      />
      <Inspector session={session} />
      <AudioMeter />
      <div
        className="splitter"
        onPointerDown={startResize}
      />
      <Timeline session={session} />
      {dropping && <div className="drop-overlay">{t("dropToImport")}</div>}
    </div>
  );
}
