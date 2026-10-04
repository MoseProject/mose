import {
  useEffect,
  useRef,
  useState,
  type PointerEvent as ReactPointerEvent,
  type ReactNode,
} from "react";
import { formatTimecode, t } from "../i18n";
import { api, frameUrl } from "../ipc";
import { Icon } from "./Icon";
import { withShortcut } from "../commands";
import { keymapStore } from "../keymap";
import { patchUi, previewStore, run, sessionStore, uiStore, useStore } from "../store";
import {
  activeTimeline,
  aspectRatioDims,
  clipDuration,
  type ClipInstance,
  type Position,
} from "../types";

/** Fits `aspect` (width/height) centered inside `avail`, matching `crates/ui/src/screens/
 *  editor/mod.rs`'s `preview_canvas_size`/layer-fit letterboxing (no zoom tool here yet, so
 *  this is always the plain "fit" case, not `preview_canvas_size`'s zoomed variants). */
function fitRect(avail: { width: number; height: number }, aspect: number) {
  let width = avail.width;
  let height = width / aspect;
  if (height > avail.height) {
    height = avail.height;
    width = height * aspect;
  }
  return {
    width,
    height,
    left: (avail.width - width) / 2,
    top: (avail.height - height) / 2,
  };
}

type DragState =
  | { mode: "move"; startClientX: number; startClientY: number; startPos: Position }
  | {
      mode: "resize";
      startClientX: number;
      startClientY: number;
      startScaleX: number;
      startScaleY: number;
    };

export function PreviewPanel() {
  const tick = useStore(previewStore);
  const session = useStore(sessionStore);
  const ui = useStore(uiStore);
  useStore(keymapStore);
  const monitorRef = useRef<HTMLDivElement>(null);
  const [monitorSize, setMonitorSize] = useState({ width: 0, height: 0 });
  const [drag, setDrag] = useState<DragState | null>(null);
  const [ghostPos, setGhostPos] = useState<Position | null>(null);
  const [ghostScale, setGhostScale] = useState<{ x: number; y: number } | null>(null);

  useEffect(() => {
    const el = monitorRef.current;
    if (!el) {
      return;
    }
    const observer = new ResizeObserver(([entry]) => {
      const { width, height } = entry.contentRect;
      setMonitorSize({ width, height });
    });
    observer.observe(el);
    return () => {
      return observer.disconnect();
    };
  }, []);

  const timecodeFps =
    session?.project.media_library.find((item) => {
      return item.kind === "Video" && item.fps;
    })?.fps ?? 30;
  const duration = session
    ? Math.max(
        0,
        ...activeTimeline(session.project).tracks.flatMap((track) => {
          return track.clips.map((clip) => {
            return clip.start_secs + clipDuration(clip);
          });
        }),
      )
    : 0;

  let clip: ClipInstance | undefined;
  let asset: { resolution: [number, number] | null } | undefined;
  if (session && ui.selectedClipIds.length === 1) {
    clip = activeTimeline(session.project)
      .tracks.flatMap((track) => {
        return track.clips;
      })
      .find((c) => {
        return c.id === ui.selectedClipIds[0];
      });
    asset =
      clip &&
      session.project.media_library.find((a) => {
        return a.id === clip!.asset_id;
      });
  }

  // Canvas rect (letterboxed to the export aspect ratio within the monitor) and the video's own
  // fit-preserving-aspect rect inside it -- shared by the layer-transform overlay, its drag
  // math, and the custom-shape drawing surface below, all of which need the same canvas-fraction
  // <-> pixel mapping.
  const texAspect = asset?.resolution ? asset.resolution[0] / asset.resolution[1] : 16 / 9;
  const aspectRatio = session
    ? session.project.sequences[session.project.active_sequence].export_settings.aspect_ratio
    : "Original";
  const [canvasW, canvasH] = aspectRatioDims(aspectRatio, asset?.resolution ?? [1920, 1080]);
  const zoomScale = ui.monitorZoom === "fit" ? null : Number(ui.monitorZoom) / 100;
  const canvasRect =
    zoomScale === null
      ? fitRect(monitorSize, canvasW / canvasH)
      : {
          width: canvasW * zoomScale,
          height: canvasH * zoomScale,
          left: (monitorSize.width - canvasW * zoomScale) / 2,
          top: (monitorSize.height - canvasH * zoomScale) / 2,
        };
  const base = fitRect({ width: canvasRect.width, height: canvasRect.height }, texAspect);
  const hasCanvas = monitorSize.width > 0 && monitorSize.height > 0;

  const commitMove = (pos: Position) => {
    if (!clip) {
      return;
    }
    void run(api.updateClip(clip.id, { positionKeyframes: [{ time_fraction: 0, value: pos }] }));
  };
  const commitResize = (scaleX: number, scaleY: number) => {
    if (!clip) {
      return;
    }
    void run(api.updateClip(clip.id, { layerScaleX: scaleX, layerScaleY: scaleY }));
  };

  const onLayerPointerDown = (e: ReactPointerEvent, position: Position) => {
    e.currentTarget.setPointerCapture(e.pointerId);
    setDrag({
      mode: "move",
      startClientX: e.clientX,
      startClientY: e.clientY,
      startPos: position,
    });
  };
  const onHandlePointerDown = (e: ReactPointerEvent, scaleX: number, scaleY: number) => {
    e.stopPropagation();
    e.currentTarget.setPointerCapture(e.pointerId);
    setDrag({
      mode: "resize",
      startClientX: e.clientX,
      startClientY: e.clientY,
      startScaleX: scaleX,
      startScaleY: scaleY,
    });
  };

  const onCanvasClick = (e: ReactPointerEvent) => {
    if (!hasCanvas) {
      return;
    }
    const rect = e.currentTarget.getBoundingClientRect();
    const fracX = Math.min(
      1,
      Math.max(0, (e.clientX - rect.left - canvasRect.left) / Math.max(1, canvasRect.width)),
    );
    const fracY = Math.min(
      1,
      Math.max(0, (e.clientY - rect.top - canvasRect.top) / Math.max(1, canvasRect.height)),
    );

    if (ui.drawingShapePoints !== null) {
      patchUi({ drawingShapePoints: [...ui.drawingShapePoints, [fracX, fracY]] });
      return;
    }
    if (ui.tool === "text") {
      void run(api.addTextClip(t("defaultTextContent"), fracX, fracY)).then((id) => {
        if (id != null) {
          patchUi({ selectedClipIds: [id] });
        }
        patchUi({ tool: "select" });
      });
    }
  };

  let body: ReactNode = null;
  if (tick.hasClip && tick.error) {
    body = (
      <span
        className="muted"
        title={tick.error}
      >
        {t("previewUnavailable")}
      </span>
    );
  } else if (!tick.hasClip) {
    body = <span className="muted">{t("previewEmpty")}</span>;
  }

  let overlay: ReactNode = null;
  if (
    tick.hasClip &&
    tick.frameSeq > 0 &&
    clip &&
    hasCanvas &&
    ui.drawingShapePoints === null &&
    ui.tool !== "text" &&
    zoomScale === null
  ) {
    const position = ghostPos ?? clip.position_keyframes[0]?.value ?? { x: 0, y: 0 };
    const scaleX = ghostScale?.x ?? clip.layer_scale_x;
    const scaleY = ghostScale?.y ?? clip.layer_scale_y;
    const layerW = base.width * scaleX;
    const layerH = base.height * scaleY;
    const layerLeft = canvasRect.left + position.x * canvasRect.width;
    const layerTop = canvasRect.top + position.y * canvasRect.height;

    const draggable = clip.position_keyframes.length <= 1;

    overlay = (
      <div
        className="preview-canvas"
        style={{
          left: canvasRect.left,
          top: canvasRect.top,
          width: canvasRect.width,
          height: canvasRect.height,
        }}
      >
        <div
          data-testid="preview-layer"
          className={`preview-layer ${draggable ? "draggable" : "animated"}`}
          style={{
            left: layerLeft - canvasRect.left,
            top: layerTop - canvasRect.top,
            width: layerW,
            height: layerH,
          }}
          onPointerDown={
            draggable
              ? (e) => {
                  return onLayerPointerDown(e, position);
                }
              : undefined
          }
          title={draggable ? t("layerTransformDragHint") : t("layerTransformAnimatedHint")}
        >
          {draggable && (
            <div
              data-testid="preview-layer-resize-handle"
              className="preview-layer-handle"
              onPointerDown={(e) => {
                return onHandlePointerDown(e, scaleX, scaleY);
              }}
              title={t("layerTransformResizeHint")}
            />
          )}
        </div>
      </div>
    );
  }

  let drawingOverlay: ReactNode = null;
  if (ui.drawingShapePoints !== null && hasCanvas) {
    const points = ui.drawingShapePoints;
    const toPx = ([x, y]: [number, number]) => {
      return [canvasRect.left + x * canvasRect.width, canvasRect.top + y * canvasRect.height];
    };
    const pixelPoints = points.map(toPx);
    const pathD =
      pixelPoints.length > 0
        ? `M ${pixelPoints
            .map(([x, y]) => {
              return `${x} ${y}`;
            })
            .join(" L ")}${points.length >= 3 ? " Z" : ""}`
        : "";
    drawingOverlay = (
      <div
        className="preview-canvas"
        data-testid="shape-drawing-surface"
        style={{
          left: 0,
          top: 0,
          width: monitorSize.width,
          height: monitorSize.height,
          pointerEvents: "none",
        }}
      >
        <svg
          width={monitorSize.width}
          height={monitorSize.height}
          style={{ position: "absolute", left: 0, top: 0 }}
        >
          {pathD && (
            <path
              d={pathD}
              fill="none"
              stroke="var(--accent)"
              strokeWidth={1.5}
              strokeDasharray={points.length < 3 ? undefined : "4 3"}
            />
          )}
          {pixelPoints.map(([x, y], i) => {
            return (
              <circle
                key={i}
                cx={x}
                cy={y}
                r={4}
                fill="var(--accent)"
              />
            );
          })}
        </svg>
        <div className="drawing-shape-hint">
          {t("drawCustomShapeHint")} ({points.length} {points.length === 1 ? "pt" : "pts"})
        </div>
      </div>
    );
  }

  const onOverlayPointerMove = (e: ReactPointerEvent) => {
    if (!drag || !clip) {
      return;
    }
    if (drag.mode === "move") {
      const dx = (e.clientX - drag.startClientX) / Math.max(1, canvasRect.width);
      const dy = (e.clientY - drag.startClientY) / Math.max(1, canvasRect.height);
      // Clamped to one full canvas width/height of overflow on either side, same bound
      // crates/ui/src/screens/editor/mod.rs's layer_transform_preview uses -- generous enough
      // for a slide-in/out animation's endpoints, without letting the value wander off to
      // where the layer becomes unreachable to drag back.
      setGhostPos({
        x: Math.min(2, Math.max(-1, drag.startPos.x + dx)),
        y: Math.min(2, Math.max(-1, drag.startPos.y + dy)),
      });
    } else {
      const baseW = Math.max(1, base.width);
      const baseH = Math.max(1, base.height);
      const dx = (e.clientX - drag.startClientX) / baseW;
      const dy = (e.clientY - drag.startClientY) / baseH;
      setGhostScale({
        x: Math.max(0.05, drag.startScaleX + dx),
        y: Math.max(0.05, drag.startScaleY + dy),
      });
    }
  };
  const onOverlayPointerUp = (e: ReactPointerEvent) => {
    if (drag) {
      if (drag.mode === "move" && ghostPos) {
        commitMove(ghostPos);
      }
      if (drag.mode === "resize" && ghostScale) {
        commitResize(ghostScale.x, ghostScale.y);
      }
      setDrag(null);
      setGhostPos(null);
      setGhostScale(null);
      return;
    }
    onCanvasClick(e);
  };

  return (
    <section className="panel preview">
      <div className="panel-title">
        <span>{t("programMonitor")}</span>
        <span className="monitor-tools">
          {(["50", "fit", "100"] as const).map((zoom) => {
            return (
              <button
                key={zoom}
                className={"chip " + (ui.monitorZoom === zoom ? "active" : "")}
                aria-pressed={ui.monitorZoom === zoom}
                title={t("monitorZoom")}
                onClick={() => {
                  patchUi({ monitorZoom: zoom });
                }}
              >
                {zoom === "fit" ? t("zoomFitShort") : zoom + "%"}
              </button>
            );
          })}
          <button
            className={"icon " + (ui.monitorGuides ? "active" : "")}
            aria-label={t("cmdToggleGuides")}
            aria-pressed={ui.monitorGuides}
            title={withShortcut(t("cmdToggleGuides"), "toggleGuides")}
            onClick={() => {
              patchUi({ monitorGuides: !ui.monitorGuides });
            }}
          >
            <Icon name="grid" />
          </button>
        </span>
      </div>
      <div
        ref={monitorRef}
        className="monitor"
        data-testid="preview-monitor"
        style={
          ui.tool === "text" || ui.drawingShapePoints !== null ? { cursor: "crosshair" } : undefined
        }
        title={
          ui.tool === "text"
            ? t("textToolHint")
            : ui.drawingShapePoints !== null
              ? t("drawCustomShapeHint")
              : undefined
        }
        onPointerMove={onOverlayPointerMove}
        onPointerUp={onOverlayPointerUp}
      >
        {tick.hasClip && tick.frameSeq > 0 && !tick.error && (
          <img
            style={
              zoomScale === null
                ? undefined
                : {
                    position: "absolute",
                    maxWidth: "none",
                    maxHeight: "none",
                    left: canvasRect.left + base.left,
                    top: canvasRect.top + base.top,
                    width: base.width,
                    height: base.height,
                  }
            }
            src={frameUrl(tick.frameSeq)}
            alt=""
            draggable={false}
          />
        )}
        {ui.monitorGuides && hasCanvas && (
          <div
            className="monitor-guides"
            data-testid="monitor-guides"
            aria-hidden="true"
            style={{
              left: canvasRect.left,
              top: canvasRect.top,
              width: canvasRect.width,
              height: canvasRect.height,
            }}
          >
            <span className="guide-safe action" />
            <span className="guide-safe title" />
            <span className="guide-line v" />
            <span className="guide-line h" />
          </div>
        )}
        {body}
        {overlay}
        {drawingOverlay}
      </div>
      <div className="transport">
        <div className="scrub">
          <span className="mono time-now">{formatTimecode(tick.playheadSecs, timecodeFps)}</span>
          <input
            type="range"
            className="scrubber"
            aria-label={t("seek")}
            min={0}
            max={Math.max(duration, 0.01)}
            step={0.01}
            value={Math.min(tick.playheadSecs, Math.max(duration, 0.01))}
            style={
              {
                "--fill": `${duration > 0 ? Math.min(100, (tick.playheadSecs / duration) * 100) : 0}%`,
              } as React.CSSProperties
            }
            onChange={(e) => {
              return void run(api.seek(Number(e.target.value)));
            }}
          />
          <span className="mono muted time-total">{formatTimecode(duration, timecodeFps)}</span>
        </div>
        <div className="transport-controls">
          <button
            className="icon"
            title={t("goToStart")}
            aria-label={t("goToStart")}
            onClick={() => {
              return run(api.seek(0));
            }}
          >
            <Icon
              name="skipBack"
              size={18}
            />
          </button>
          <button
            className="icon"
            title={t("stepBack")}
            aria-label={t("stepBack")}
            onClick={() => {
              return run(api.step(-1));
            }}
          >
            <Icon
              name="stepBack"
              size={18}
            />
          </button>
          <button
            className="play"
            title={tick.playing ? t("pause") : t("play")}
            aria-label={tick.playing ? t("pause") : t("play")}
            onClick={() => {
              return run(api.setPlaying(!tick.playing));
            }}
          >
            <Icon
              name={tick.playing ? "pause" : "play"}
              filled
              size={20}
            />
          </button>
          <button
            className="icon"
            title={t("stepForward")}
            aria-label={t("stepForward")}
            onClick={() => {
              return run(api.step(1));
            }}
          >
            <Icon
              name="stepForward"
              size={18}
            />
          </button>
          <button
            className="icon"
            title={t("goToEnd")}
            aria-label={t("goToEnd")}
            onClick={() => {
              return run(api.seek(duration));
            }}
          >
            <Icon
              name="skipForward"
              size={18}
            />
          </button>
        </div>
      </div>
    </section>
  );
}
