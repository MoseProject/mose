import { cx } from "../cx";
import baseStyles from "../styles/base.module.css";
import styles from "./editor.module.css";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { AudioMeter } from "../editor/chrome/AudioMeter";
import { ToolRail } from "../editor/ToolRail";
import { Inspector } from "../editor/inspector/Inspector";
import { MediaLibrary } from "../editor/MediaLibrary";
import { PreviewPanel } from "../editor/PreviewPanel";
import { Timeline } from "../editor/timeline/Timeline";
import { t } from "../i18n";
import { api } from "../ipc";
import { commandForEvent } from "../commands";
import { isActivatableTarget, shouldIgnoreShortcut } from "../keyboard";
import { stopRange, stopShuttle } from "../shuttle";
import { patchUi, run, uiStore } from "../store";
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

  // Timers of a running shuttle or range playback must not outlive the editor.
  useEffect(() => {
    return () => {
      stopShuttle();
      stopRange();
    };
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (shouldIgnoreShortcut(e, uiStore.get().openDialogs)) {
        return;
      }
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
      if (e.code === "Space" && isActivatableTarget(e.target)) {
        return;
      }
      const command = commandForEvent(e);
      if (!command) {
        return;
      }
      if (e.repeat && !command.repeatable) {
        e.preventDefault();
        return;
      }
      if (!command.enabled || command.enabled()) {
        void command.run();
      }
      e.preventDefault();
    };
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("keydown", onKey);
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
      className={cx(styles, baseStyles, "editor")}
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
        className={cx(styles, baseStyles, "panel-splitter library-splitter")}
        onPointerDown={(e) => {
          startPanelResize("library")(e);
        }}
      />
      <PreviewPanel />
      <div
        className={cx(styles, baseStyles, "panel-splitter inspector-splitter")}
        onPointerDown={(e) => {
          startPanelResize("inspector")(e);
        }}
      />
      <Inspector session={session} />
      <AudioMeter />
      <div
        className={cx(styles, baseStyles, "splitter")}
        onPointerDown={startResize}
      />
      <Timeline session={session} />
      {dropping && <div className={cx(styles, baseStyles, "drop-overlay")}>{t("dropToImport")}</div>}
    </div>
  );
}
