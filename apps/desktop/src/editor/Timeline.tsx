import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { formatTime, t } from "../i18n";
import { api } from "../ipc";
import { LayerTemplates } from "./LayerTemplates";
import {
  assetDragStore,
  MAX_PX_PER_SEC,
  MIN_PX_PER_SEC,
  patchUi,
  previewStore,
  run,
  uiStore,
  useStore,
  type EditorTool,
} from "../store";
import {
  HEADER_WIDTH,
  RULER_HEIGHT,
  SNAP_PX,
  TRACK_HEIGHT,
  hitTest,
  markerAtX,
  rulerTicks,
  snap,
  snapCandidates,
  findSpan,
  timeToX,
  trackIndexAtY,
  xToTime,
  zoomAround,
} from "../timelineGeometry";
import {
  activeTimeline,
  clipDuration,
  type ClipInstance,
  type MediaAsset,
  type SessionView,
  type ShapeClip,
  type TextClip,
  type Track,
} from "../types";

/** Seeks sent while scrubbing the ruler are coalesced to preview cadence (egui: 33 ms). */
const SCRUB_SEEK_INTERVAL_MS = 33;
const TOOLS: [EditorTool, "toolSelect" | "toolRoll" | "toolSlip" | "toolSlide"][] = [
  ["select", "toolSelect"],
  ["roll", "toolRoll"],
  ["slip", "toolSlip"],
  ["slide", "toolSlide"],
];
const UI_FONT = "11px 'Segoe UI', system-ui, sans-serif";
const MONO_FONT = "11px 'Cascadia Mono', Consolas, monospace";

type Drag =
  | { mode: "scrub" }
  | {
      mode: "move";
      clipId: number;
      grabOffsetSecs: number;
      ghostStart: number;
      ghostTrack: number;
      duration: number;
      /** "slide" keeps the clip's own range and absorbs the move into its neighbors instead of
       *  repositioning; the row never changes for it. */
      editKind: "move" | "slide";
    }
  | {
      mode: "trim-start" | "trim-end";
      clipId: number;
      ghostEdge: number;
      /** Rolls the shared boundary with the neighbor on that side instead of trimming alone. */
      roll: boolean;
    }
  | {
      mode: "slip";
      clipId: number;
      grabX: number;
      deltaSecs: number;
    };

interface Palette {
  bg: string;
  surface: string;
  surface2: string;
  border: string;
  text: string;
  textMuted: string;
  accent: string;
  video: string;
  audio: string;
  playhead: string;
}

function readPalette(): Palette {
  const css = getComputedStyle(document.documentElement);
  const v = (name: string) => css.getPropertyValue(name).trim();
  return {
    bg: v("--bg-workspace"),
    surface: v("--surface"),
    surface2: v("--surface-2"),
    border: v("--border-subtle"),
    text: v("--text-primary"),
    textMuted: v("--text-muted"),
    accent: v("--accent"),
    video: v("--media-video"),
    audio: v("--audio-tint"),
    playhead: v("--playhead"),
  };
}

/** Snaps a moved clip by whichever of its edges lands on a candidate with less adjustment. */
function snapMove(raw: number, duration: number, candidates: number[], threshold: number) {
  const byStart = snap(raw, candidates, threshold);
  const byEnd = snap(raw + duration, candidates, threshold) - duration;
  const startSnapped = byStart !== raw;
  const endSnapped = byEnd !== raw;
  if (startSnapped && (!endSnapped || Math.abs(byStart - raw) <= Math.abs(byEnd - raw))) {
    return byStart;
  }
  return endSnapped ? Math.max(0, byEnd) : raw;
}

export function Timeline({ session }: { session: SessionView }) {
  const ui = useStore(uiStore);
  const tracks = activeTimeline(session.project).tracks;
  const markers = activeTimeline(session.project).markers;
  const media = session.project.media_library;
  const [scrollSecs, setScrollSecs] = useState(0);
  const [scrollY, setScrollY] = useState(0);
  const [cursor, setCursor] = useState("default");

  const lanesRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const dragRef = useRef<Drag | null>(null);
  const sizeRef = useRef({ width: 0, height: 0 });
  const paletteRef = useRef<Palette | null>(null);
  const frameRef = useRef(0);
  const lastScrubSeekRef = useRef(0);

  // Everything draw() reads, refreshed every render so the rAF callback never sees stale data.
  const stateRef = useRef({ tracks, markers, media, ui, scrollSecs, scrollY });
  stateRef.current = { tracks, markers, media, ui, scrollSecs, scrollY };

  const draw = () => {
    const canvas = canvasRef.current;
    const ctx = canvas?.getContext("2d");
    if (!canvas || !ctx) return;
    const palette = (paletteRef.current ??= readPalette());
    const { width, height } = sizeRef.current;
    const { tracks, markers, media, ui, scrollSecs, scrollY } = stateRef.current;
    const px = ui.pxPerSec;
    const dpr = window.devicePixelRatio || 1;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.fillStyle = palette.bg;
    ctx.fillRect(0, 0, width, height);

    // Track rows, clipped below the ruler and offset by the vertical scroll.
    ctx.save();
    ctx.beginPath();
    ctx.rect(0, RULER_HEIGHT, width, height - RULER_HEIGHT);
    ctx.clip();
    ctx.translate(0, -scrollY);
    const drag = dragRef.current;
    tracks.forEach((track, index) => {
      const y = RULER_HEIGHT + index * TRACK_HEIGHT;
      if (y + TRACK_HEIGHT < RULER_HEIGHT + scrollY || y > height + scrollY) return;
      ctx.fillStyle = index % 2 === 0 ? palette.surface : palette.bg;
      ctx.fillRect(0, y, width, TRACK_HEIGHT);
      ctx.fillStyle = palette.border;
      ctx.fillRect(0, y + TRACK_HEIGHT - 1, width, 1);
      for (const clip of track.clips) {
        let start = clip.start_secs;
        let end = start + clipDuration(clip);
        if (drag && drag.mode !== "scrub" && drag.clipId === clip.id) {
          if (drag.mode === "move") continue; // drawn as a ghost below
          if (drag.mode === "trim-start") start = Math.min(drag.ghostEdge, end - 0.1);
          if (drag.mode === "trim-end") end = Math.max(drag.ghostEdge, start + 0.1);
        }
        const left = timeToX(start, px, scrollSecs);
        const right = timeToX(end, px, scrollSecs);
        if (right < 0 || left > width) continue; // horizontal culling before any per-clip work
        const asset = media.find((a) => a.id === clip.asset_id);
        drawClip(ctx, palette, track, clip, asset, left, right, y, width, ui.selectedClipIds, 1);
      }
      for (const clip of [...track.text_clips, ...track.shape_clips]) {
        let start = clip.start_secs;
        let end = start + clip.duration_secs;
        if (drag && drag.mode !== "scrub" && drag.clipId === clip.id) {
          if (drag.mode === "move") continue; // drawn as a ghost below
          if (drag.mode === "trim-start") start = Math.min(drag.ghostEdge, end - 0.1);
          if (drag.mode === "trim-end") end = Math.max(drag.ghostEdge, start + 0.1);
        }
        const left = timeToX(start, px, scrollSecs);
        const right = timeToX(end, px, scrollSecs);
        if (right < 0 || left > width) continue;
        drawOverlayClip(ctx, palette, clip, left, right, y, ui.selectedClipIds, 1);
      }
      if (!track.visible) {
        ctx.fillStyle = "rgba(0,0,0,0.45)";
        ctx.fillRect(0, y, width, TRACK_HEIGHT - 1);
      }
    });
    if (drag?.mode === "move") {
      const track = tracks[drag.ghostTrack];
      const y = RULER_HEIGHT + drag.ghostTrack * TRACK_HEIGHT;
      const left = timeToX(drag.ghostStart, px, scrollSecs);
      const right = timeToX(drag.ghostStart + drag.duration, px, scrollSecs);
      const clip = tracks.flatMap((t) => t.clips).find((c) => c.id === drag.clipId);
      const overlay = clip
        ? undefined
        : tracks.flatMap((t) => [...t.text_clips, ...t.shape_clips]).find((c) => c.id === drag.clipId);
      if (clip && track) {
        const asset = media.find((a) => a.id === clip.asset_id);
        drawClip(ctx, palette, track, clip, asset, left, right, y, width, [clip.id], 0.75);
      } else if (overlay && track) {
        drawOverlayClip(ctx, palette, overlay, left, right, y, [overlay.id], 0.75);
      }
    }
    const assetDrag = assetDragStore.get();
    if (assetDrag) {
      const rect = canvas.getBoundingClientRect();
      const x = assetDrag.clientX - rect.left;
      const y = assetDrag.clientY - rect.top + scrollY;
      const asset = media.find((a) => a.id === assetDrag.assetId);
      if (asset && x >= 0 && x <= width && y >= RULER_HEIGHT) {
        const rowY = RULER_HEIGHT + Math.floor((y - RULER_HEIGHT) / TRACK_HEIGHT) * TRACK_HEIGHT;
        ctx.globalAlpha = 0.35;
        ctx.fillStyle = palette.accent;
        ctx.fillRect(x, rowY + 4, asset.duration_secs * px, TRACK_HEIGHT - 9);
        ctx.globalAlpha = 1;
      }
    }
    ctx.restore();

    // Ruler.
    ctx.fillStyle = palette.surface2;
    ctx.fillRect(0, 0, width, RULER_HEIGHT);
    ctx.fillStyle = palette.border;
    ctx.fillRect(0, RULER_HEIGHT - 1, width, 1);
    ctx.font = MONO_FONT;
    ctx.textBaseline = "middle";
    ctx.fillStyle = palette.textMuted;
    for (const tick of rulerTicks(px, scrollSecs, width)) {
      const x = Math.round(timeToX(tick, px, scrollSecs)) + 0.5;
      ctx.fillRect(x, RULER_HEIGHT - 8, 1, 7);
      ctx.fillText(formatTime(tick).replace(/\.00$/, ""), x + 4, RULER_HEIGHT / 2);
    }
    for (const marker of markers) {
      const x = timeToX(marker.position_secs, px, scrollSecs);
      if (x < -8 || x > width + 8) continue;
      ctx.fillStyle = marker.completed ? palette.textMuted : palette.accent;
      ctx.beginPath();
      ctx.moveTo(x, 1);
      ctx.lineTo(x + 6, 1);
      ctx.lineTo(x, 8);
      ctx.fill();
    }

    // Playhead.
    const playheadX = timeToX(previewStore.get().playheadSecs, px, scrollSecs);
    if (playheadX >= -6 && playheadX <= width + 6) {
      ctx.fillStyle = palette.playhead;
      ctx.fillRect(Math.round(playheadX), 0, 1, height);
      ctx.beginPath();
      ctx.moveTo(playheadX - 5, 0);
      ctx.lineTo(playheadX + 6, 0);
      ctx.lineTo(playheadX + 0.5, 8);
      ctx.fill();
    }
  };

  const drawRef = useRef(draw);
  drawRef.current = draw;
  const scheduleDraw = useRef(() => {
    if (frameRef.current) return;
    frameRef.current = requestAnimationFrame(() => {
      frameRef.current = 0;
      drawRef.current();
    });
  }).current;

  // Canvas sizing follows the lanes container.
  useLayoutEffect(() => {
    const lanes = lanesRef.current;
    const canvas = canvasRef.current;
    if (!lanes || !canvas) return;
    const observer = new ResizeObserver(() => {
      const dpr = window.devicePixelRatio || 1;
      const width = lanes.clientWidth;
      const height = lanes.clientHeight;
      sizeRef.current = { width, height };
      canvas.width = Math.round(width * dpr);
      canvas.height = Math.round(height * dpr);
      canvas.style.width = `${width}px`;
      canvas.style.height = `${height}px`;
      scheduleDraw();
    });
    observer.observe(lanes);
    return () => observer.disconnect();
  }, [scheduleDraw]);

  // Playhead ticks and library drags repaint without re-rendering React.
  useEffect(() => {
    const stopPreview = previewStore.subscribe(scheduleDraw);
    const stopAssetDrag = assetDragStore.subscribe(scheduleDraw);
    return () => {
      stopPreview();
      stopAssetDrag();
      cancelAnimationFrame(frameRef.current);
      frameRef.current = 0;
    };
  }, [scheduleDraw]);

  useEffect(() => scheduleDraw());

  // Library-to-timeline drops.
  useEffect(() => {
    const onUp = (e: PointerEvent) => {
      const drag = assetDragStore.get();
      const canvas = canvasRef.current;
      if (!drag || !canvas) return;
      const rect = canvas.getBoundingClientRect();
      const x = e.clientX - rect.left;
      const y = e.clientY - rect.top;
      if (x < 0 || y < RULER_HEIGHT || x > rect.width || y > rect.height) return;
      const { tracks, ui, scrollSecs, scrollY } = stateRef.current;
      const index = trackIndexAtY(y + scrollY, tracks.length);
      const start = Math.max(0, xToTime(x, ui.pxPerSec, scrollSecs));
      void run(api.addAssetToTimeline(drag.assetId, tracks[index]?.id, start));
    };
    window.addEventListener("pointerup", onUp);
    return () => window.removeEventListener("pointerup", onUp);
  }, []);

  const lanePoint = (e: { clientX: number; clientY: number }) => {
    const rect = canvasRef.current!.getBoundingClientRect();
    return { x: e.clientX - rect.left, y: e.clientY - rect.top };
  };

  const scrubTo = (x: number, final: boolean) => {
    const secs = Math.max(0, xToTime(x, ui.pxPerSec, scrollSecs));
    const now = performance.now();
    if (final || now - lastScrubSeekRef.current >= SCRUB_SEEK_INTERVAL_MS) {
      lastScrubSeekRef.current = now;
      void run(api.seek(secs));
    }
  };

  const onPointerDown = (e: React.PointerEvent<HTMLCanvasElement>) => {
    if (e.button !== 0) return;
    const { x, y } = lanePoint(e);
    e.currentTarget.setPointerCapture(e.pointerId);
    if (y < RULER_HEIGHT) {
      const marker = markerAtX(markers, x, ui.pxPerSec, scrollSecs);
      if (marker) {
        void run(api.seek(marker.position_secs));
        return;
      }
      dragRef.current = { mode: "scrub" };
      scrubTo(x, true);
      return;
    }
    const hit = hitTest(tracks, x, y + scrollY, ui.pxPerSec, scrollSecs);
    if (!hit) {
      patchUi({ selectedClipIds: [] });
      return;
    }
    const selected = ui.selectedClipIds;
    if (e.ctrlKey || e.metaKey) {
      patchUi({
        selectedClipIds: selected.includes(hit.clipId)
          ? selected.filter((id) => id !== hit.clipId)
          : [...selected, hit.clipId],
      });
      return;
    }
    if (!selected.includes(hit.clipId)) patchUi({ selectedClipIds: [hit.clipId] });
    const track = tracks[hit.trackIndex];
    if (track.locked) return;
    const clip = findSpan(tracks, hit.clipId)!.span;
    const end = clip.start_secs + clip.duration_secs;
    const tool: EditorTool = clip.overlay ? "select" : ui.tool;
    if (tool === "slip" && hit.part === "body") {
      dragRef.current = { mode: "slip", clipId: clip.id, grabX: x, deltaSecs: 0 };
      return;
    }
    if (tool === "roll" && hit.part !== "body") {
      dragRef.current = {
        mode: hit.part === "start" ? "trim-start" : "trim-end",
        clipId: clip.id,
        ghostEdge: hit.part === "start" ? clip.start_secs : end,
        roll: true,
      };
      return;
    }
    dragRef.current =
      hit.part === "body"
        ? {
            mode: "move",
            clipId: clip.id,
            grabOffsetSecs: xToTime(x, ui.pxPerSec, scrollSecs) - clip.start_secs,
            ghostStart: clip.start_secs,
            ghostTrack: hit.trackIndex,
            duration: clip.duration_secs,
            editKind: tool === "slide" ? "slide" : "move",
          }
        : {
            mode: hit.part === "start" ? "trim-start" : "trim-end",
            clipId: clip.id,
            roll: false,
            ghostEdge: hit.part === "start" ? clip.start_secs : end,
          };
  };

  const onPointerMove = (e: React.PointerEvent<HTMLCanvasElement>) => {
    const { x, y } = lanePoint(e);
    const drag = dragRef.current;
    const px = ui.pxPerSec;
    if (!drag) {
      const hit = y >= RULER_HEIGHT ? hitTest(tracks, x, y + scrollY, px, scrollSecs) : null;
      setCursor(!hit ? "default" : hit.part === "body" ? "grab" : "ew-resize");
      return;
    }
    if (drag.mode === "scrub") {
      scrubTo(x, false);
      return;
    }
    const candidates = snapCandidates(tracks, drag.clipId, [previewStore.get().playheadSecs]);
    const threshold = SNAP_PX / px;
    if (drag.mode === "move") {
      const raw = Math.max(0, xToTime(x, px, scrollSecs) - drag.grabOffsetSecs);
      drag.ghostStart = snapMove(raw, drag.duration, candidates, threshold);
      if (drag.editKind === "move") {
        const row = trackIndexAtY(y + scrollY, tracks.length);
        const source = tracks[findSpan(tracks, drag.clipId)?.trackIndex ?? -1];
        if (row >= 0 && source && tracks[row].kind === source.kind && !tracks[row].locked) {
          drag.ghostTrack = row;
        }
      }
    } else if (drag.mode === "slip") {
      drag.deltaSecs = (x - drag.grabX) / px;
    } else {
      drag.ghostEdge = snap(Math.max(0, xToTime(x, px, scrollSecs)), candidates, threshold);
    }
    scheduleDraw();
  };

  const onPointerUp = (e: React.PointerEvent<HTMLCanvasElement>) => {
    const drag = dragRef.current;
    dragRef.current = null;
    if (!drag) return;
    if (drag.mode === "scrub") {
      scrubTo(lanePoint(e).x, true);
    } else if (drag.mode === "move") {
      const found = findSpan(tracks, drag.clipId);
      const target = tracks[drag.ghostTrack];
      if (found && target) {
        const { span, trackIndex } = found;
        if (span.start_secs !== drag.ghostStart || drag.ghostTrack !== trackIndex) {
          if (span.overlay) {
            void run(api.moveOverlayClip(span.id, target.id, drag.ghostStart));
          } else if (drag.editKind === "slide") {
            void run(api.slideClip(span.id, drag.ghostStart));
          } else if (drag.ghostTrack !== trackIndex) {
            void run(api.moveClip(span.id, target.id, drag.ghostStart));
          } else {
            void run(api.moveClipGroup(span.id, drag.ghostStart));
          }
        }
      }
    } else if (drag.mode === "slip") {
      if (Math.abs(drag.deltaSecs) > 1e-6) void run(api.slipClip(drag.clipId, drag.deltaSecs));
    } else {
      const edge = drag.mode === "trim-start" ? "start" : "end";
      const overlay = findSpan(tracks, drag.clipId)?.span.overlay ?? false;
      if (overlay) {
        void run(api.trimOverlayClip(drag.clipId, edge, drag.ghostEdge));
      } else if (drag.roll) {
        void run(
          edge === "start"
            ? api.rollEditFromStartEdge(drag.clipId, drag.ghostEdge)
            : api.rollEditClip(drag.clipId, drag.ghostEdge),
        );
      } else {
        void run(api.trimClip(drag.clipId, edge, drag.ghostEdge));
      }
    }
    scheduleDraw();
  };

  const onWheel = (e: React.WheelEvent<HTMLCanvasElement>) => {
    const { x } = lanePoint(e);
    if (e.ctrlKey || e.metaKey) {
      const factor = e.deltaY < 0 ? 1.15 : 1 / 1.15;
      const zoom = zoomAround(x, ui.pxPerSec, scrollSecs, factor, MIN_PX_PER_SEC, MAX_PX_PER_SEC);
      patchUi({ pxPerSec: zoom.pxPerSec });
      setScrollSecs(zoom.scrollSecs);
      return;
    }
    const contentHeight = tracks.length * TRACK_HEIGHT;
    const viewHeight = sizeRef.current.height - RULER_HEIGHT;
    const horizontal = e.shiftKey || Math.abs(e.deltaX) > Math.abs(e.deltaY);
    if (!horizontal && contentHeight > viewHeight) {
      setScrollY((prev) => Math.max(0, Math.min(contentHeight - viewHeight, prev + e.deltaY)));
    } else {
      const delta = e.deltaX !== 0 ? e.deltaX : e.deltaY;
      setScrollSecs((prev) => Math.max(0, prev + delta / ui.pxPerSec));
    }
  };

  const zoomBy = (factor: number) => {
    const anchor = sizeRef.current.width / 2;
    const zoom = zoomAround(anchor, ui.pxPerSec, scrollSecs, factor, MIN_PX_PER_SEC, MAX_PX_PER_SEC);
    patchUi({ pxPerSec: zoom.pxPerSec });
    setScrollSecs(zoom.scrollSecs);
  };

  const selected = ui.selectedClipIds;
  return (
    <section className="panel timeline">
      <div className="timeline-toolbar">
        <span className="panel-title" style={{ border: 0, padding: "0 8px 0 0" }}>
          {t("timeline")}
        </span>
        {TOOLS.map(([tool, label]) => (
          <button
            key={tool}
            className={`icon ${ui.tool === tool ? "active" : ""}`}
            title={t(label)}
            onClick={() => patchUi({ tool })}
          >
            {t(label)}
          </button>
        ))}
        <button className="icon" onClick={() => run(api.splitAtPlayhead(selected))}>
          {t("split")}
        </button>
        <button
          className="icon"
          disabled={selected.length === 0}
          onClick={() => run(api.deleteClips(selected, false))}
        >
          {t("delete")}
        </button>
        <button
          className="icon"
          disabled={selected.length === 0}
          onClick={() => run(api.deleteClips(selected, true))}
        >
          {t("rippleDelete")}
        </button>
        <button
          className="icon"
          disabled={selected.length < 2}
          onClick={() => run(api.mergeClipsIntoComposite(selected))}
        >
          {t("mergeComposite")}
        </button>
        <LayerTemplates session={session} />
        <button className="icon" onClick={() => run(api.addTrack("Video"))}>
          {t("addVideoTrack")}
        </button>
        <button className="icon" onClick={() => run(api.addTrack("Audio"))}>
          {t("addAudioTrack")}
        </button>
        <button className="icon" onClick={() => void addOverlay(api.addTextClip(t("defaultTextContent")))}>
          {t("addTextClip")}
        </button>
        <button className="icon" onClick={() => void addOverlay(api.addShapeClip())}>
          {t("addShapeClip")}
        </button>
        <button className="icon" title={t("addMarker")} onClick={() => run(api.addMarker())}>M</button>
        <button className="icon" title={t("previousMarker")} onClick={() => {
          const previous = markers.filter((m) => m.position_secs < previewStore.get().playheadSecs).sort((a, b) => b.position_secs - a.position_secs)[0];
          if (previous) void run(api.seek(previous.position_secs));
        }}>‹M</button>
        <button className="icon" title={t("nextMarker")} onClick={() => {
          const next = markers.filter((m) => m.position_secs > previewStore.get().playheadSecs).sort((a, b) => a.position_secs - b.position_secs)[0];
          if (next) void run(api.seek(next.position_secs));
        }}>M›</button>
        <span style={{ flex: 1 }} />
        <button className="icon" title={t("zoomOut")} onClick={() => zoomBy(1 / 1.5)}>
          −
        </button>
        <button className="icon" title={t("zoomIn")} onClick={() => zoomBy(1.5)}>
          +
        </button>
      </div>
      <div className="timeline-body">
        <div className="track-headers" style={{ width: HEADER_WIDTH }}>
          <div style={{ height: RULER_HEIGHT }} />
          <div style={{ transform: `translateY(${-scrollY}px)` }}>
            {tracks.map((track) => (
              <TrackHeader key={track.id} track={track} />
            ))}
          </div>
        </div>
        <div className="timeline-lanes" ref={lanesRef}>
          <canvas
            ref={canvasRef}
            style={{ cursor }}
            onPointerDown={onPointerDown}
            onPointerMove={onPointerMove}
            onPointerUp={onPointerUp}
            onWheel={onWheel}
          />
          {tracks.length === 0 && (
            <div
              className="empty-state"
              style={{ position: "absolute", top: RULER_HEIGHT + 16, width: "100%" }}
            >
              {t("timelineEmpty")}
            </div>
          )}
        </div>
      </div>
    </section>
  );
}

/** Creates a text/shape clip at the playhead and selects it, as egui's "+ Add" buttons do. */
async function addOverlay(call: Promise<number | null>) {
  const id = await run(call);
  if (id != null) patchUi({ selectedClipIds: [id] });
}

function TrackHeader({ track }: { track: Track }) {
  return (
    <div className="track-header" style={{ height: TRACK_HEIGHT }}>
      <span className="name">{track.name}</span>
      <button
        className={`icon ${track.visible ? "" : "active"}`}
        title={track.visible ? t("hideTrack") : t("showTrack")}
        onClick={() => run(api.setTrackFlags(track.id, { visible: !track.visible }))}
      >
        {track.visible ? "◉" : "○"}
      </button>
      <button
        className={`icon ${track.locked ? "active" : ""}`}
        title={track.locked ? t("unlockTrack") : t("lockTrack")}
        onClick={() => run(api.setTrackFlags(track.id, { locked: !track.locked }))}
      >
        {track.locked ? "🔒" : "🔓"}
      </button>
    </div>
  );
}

function drawClip(
  ctx: CanvasRenderingContext2D,
  palette: Palette,
  track: Track,
  clip: ClipInstance,
  asset: MediaAsset | undefined,
  left: number,
  right: number,
  rowY: number,
  viewWidth: number,
  selectedIds: number[],
  alpha: number,
) {
  const top = rowY + 4;
  const height = TRACK_HEIGHT - 9;
  const width = Math.max(1, right - left);
  ctx.globalAlpha = alpha;
  ctx.fillStyle = clip.color_label
    ? `rgb(${clip.color_label.join(",")})`
    : track.kind === "Audio"
      ? palette.audio
      : palette.video;
  ctx.fillRect(left, top, width, height);

  if (asset && asset.waveform_peaks && asset.waveform_peaks.length > 0) {
    drawWaveform(ctx, asset, clip, left, width, top, height, viewWidth);
  }

  if (width > 24) {
    ctx.save();
    ctx.beginPath();
    ctx.rect(left + 4, top, width - 8, height);
    ctx.clip();
    ctx.fillStyle = palette.text;
    ctx.font = UI_FONT;
    ctx.textBaseline = "top";
    const label = asset?.file_name ?? `#${clip.asset_id}`;
    const badges = [clip.speed_factor !== 1 ? `${clip.speed_factor.toFixed(2)}×` : "", clip.frozen ? "❄" : ""]
      .filter(Boolean)
      .join(" ");
    ctx.fillText(badges ? `${label}  ${badges}` : label, Math.max(left, 0) + 5, top + 4);
    ctx.restore();
  }

  if (selectedIds.includes(clip.id)) {
    ctx.strokeStyle = palette.accent;
    ctx.lineWidth = 2;
    ctx.strokeRect(left + 1, top + 1, width - 2, height - 2);
  }
  ctx.globalAlpha = 1;
}

/** Text/shape overlay block, tinted with the overlay's own color like egui's timeline. */
function drawOverlayClip(
  ctx: CanvasRenderingContext2D,
  palette: Palette,
  clip: TextClip | ShapeClip,
  left: number,
  right: number,
  rowY: number,
  selectedIds: number[],
  alpha: number,
) {
  const top = rowY + 4;
  const height = TRACK_HEIGHT - 9;
  const width = Math.max(1, right - left);
  const [r, g, b] = clip.color_rgba;
  ctx.globalAlpha = alpha;
  ctx.fillStyle = palette.surface2;
  ctx.fillRect(left, top, width, height);
  ctx.fillStyle = `rgba(${r},${g},${b},${120 / 255})`;
  ctx.fillRect(left, top, width, height);
  if (width > 24) {
    ctx.save();
    ctx.beginPath();
    ctx.rect(left + 4, top, width - 8, height);
    ctx.clip();
    ctx.fillStyle = palette.text;
    ctx.font = UI_FONT;
    ctx.textBaseline = "top";
    const label = "text" in clip ? clip.text || t("text") : t("shape");
    ctx.fillText(label, Math.max(left, 0) + 5, top + 4);
    ctx.restore();
  }
  if (selectedIds.includes(clip.id)) {
    ctx.strokeStyle = palette.accent;
    ctx.lineWidth = 2;
    ctx.strokeRect(left + 1, top + 1, width - 2, height - 2);
  }
  ctx.globalAlpha = 1;
}

/** Peaks cover the asset's whole duration; draw only the clip's trimmed source window, and only
 *  the columns that are on screen. */
function drawWaveform(
  ctx: CanvasRenderingContext2D,
  asset: MediaAsset,
  clip: ClipInstance,
  left: number,
  width: number,
  top: number,
  height: number,
  viewWidth: number,
) {
  const peaks = asset.waveform_peaks!;
  if (asset.duration_secs <= 0) return;
  const gain = Math.pow(10, clip.gain_db / 20);
  const mid = top + height / 2;
  const half = height / 2 - 2;
  const perBucket = asset.duration_secs / peaks.length;
  const sourceSpan = clip.source_out_secs - clip.source_in_secs;
  const firstColumn = Math.max(0, Math.floor(-left));
  const lastColumn = Math.min(width, viewWidth - left);
  ctx.fillStyle = "rgba(255,255,255,0.28)";
  for (let column = firstColumn; column < lastColumn; column += 1) {
    const secs = clip.source_in_secs + (column / width) * sourceSpan;
    const bucket = peaks[Math.min(peaks.length - 1, Math.floor(secs / perBucket))];
    if (!bucket) continue;
    const high = Math.min(1, bucket[1] * gain) * half;
    const low = Math.max(-1, bucket[0] * gain) * half;
    ctx.fillRect(left + column, mid - high, 1, Math.max(1, high - low));
  }
}
