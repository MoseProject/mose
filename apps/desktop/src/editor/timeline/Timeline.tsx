import chromeStyles from "../chrome/chrome.module.css";
import libraryStyles from "../library/library.module.css";
import editorStyles from "../../screens/editor.module.css";
import { cx } from "../../cx";
import baseStyles from "../../styles/base.module.css";
import styles from "./timeline.module.css";
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import { formatTime, t } from "../../i18n";
import { findCommand, shortcutLabel } from "../../commands";
import { TIMELINE_ACTION_EVENT, type TimelineAction } from "../../editing";
import { api } from "../../ipc";
import { ContextMenu, type ContextMenuItem } from "../chrome/ContextMenu";
import { TimelineMinimap } from "./TimelineMinimap";
import { MEDIA_EVENT, syncAssets } from "./timelineMedia";
import {
  assetDragStore,
  effectDragStore,
  MAX_PX_PER_SEC,
  MIN_PX_PER_SEC,
  patchUi,
  previewStore,
  run,
  uiStore,
  useStore,
  type EditorTool,
} from "../../store";
import {
  effectTarget,
  HEADER_WIDTH,
  RULER_HEIGHT,
  SNAP_PX,
  TRACK_HEIGHT,
  hitTest,
  markerAtX,
  fitPxPerSec,
  rulerLabel,
  rulerTicks,
  snap,
  snapCandidates,
  findSpan,
  timeToX,
  trackIndexAtY,
  xToTime,
  zoomAround,
} from "../../timelineGeometry";
import { activeTimeline, clipDuration, type SessionView, type Track } from "../../types";
import { TrackHeader } from "./TrackHeader";
import { type Drag, snapMove } from "./timelineDrag";
import { MONO_FONT, drawClip, drawOverlayClip, readPalette, type Palette } from "./timelinePaint";
import { TimelineToolbar } from "./TimelineToolbar";

/** Seeks sent while scrubbing the ruler are coalesced to preview cadence (egui: 33 ms). */
const SCRUB_SEEK_INTERVAL_MS = 33;

export function Timeline({ session }: { session: SessionView }) {
  const ui = useStore(uiStore);
  const tracks = activeTimeline(session.project).tracks;
  const markers = activeTimeline(session.project).markers;
  const media = session.project.media_library;
  const [scrollSecs, setScrollSecs] = useState(0);
  const [scrollY, setScrollY] = useState(0);
  const [cursor, setCursor] = useState("default");
  const [viewWidth, setViewWidth] = useState(0);
  const [menu, setMenu] = useState<{ x: number; y: number; items: ContextMenuItem[] } | null>(null);

  const lanesRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const dragRef = useRef<Drag | null>(null);
  const sizeRef = useRef({ width: 0, height: 0 });
  const paletteRef = useRef<Palette | null>(null);
  const frameRef = useRef(0);
  const lastScrubSeekRef = useRef(0);

  // Everything draw() reads, refreshed every render so the rAF callback never sees stale data.
  // Cached frames and waveforms belong to an asset id; a new project reuses the ids.
  useLayoutEffect(() => {
    syncAssets(media);
  }, [media]);
  const stateRef = useRef({ tracks, markers, media, ui, scrollSecs, scrollY });
  useLayoutEffect(() => {
    stateRef.current = { tracks, markers, media, ui, scrollSecs, scrollY };
  });

  const draw = () => {
    const canvas = canvasRef.current;
    const ctx = canvas?.getContext("2d");
    if (!canvas || !ctx) {
      return;
    }
    paletteRef.current ??= readPalette();
    const palette = paletteRef.current;
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
      if (y + TRACK_HEIGHT < RULER_HEIGHT + scrollY || y > height + scrollY) {
        return;
      }
      ctx.fillStyle = index % 2 === 0 ? palette.surface : palette.bg;
      ctx.fillRect(0, y, width, TRACK_HEIGHT);
      ctx.fillStyle = palette.border;
      ctx.fillRect(0, y + TRACK_HEIGHT - 1, width, 1);
      for (const clip of track.clips) {
        let start = clip.start_secs;
        let end = start + clipDuration(clip);
        if (drag && drag.mode !== "scrub" && drag.clipId === clip.id) {
          if (drag.mode === "move") {
            continue;
          } // drawn as a ghost below
          if (drag.mode === "trim-start") {
            start = Math.min(drag.ghostEdge, end - 0.1);
          }
          if (drag.mode === "trim-end") {
            end = Math.max(drag.ghostEdge, start + 0.1);
          }
        }
        const left = timeToX(start, px, scrollSecs);
        const right = timeToX(end, px, scrollSecs);
        if (right < 0 || left > width) {
          continue;
        } // horizontal culling before any per-clip work
        const asset = media.find((a) => {
          return a.id === clip.asset_id;
        });
        drawClip(ctx, palette, track, clip, asset, left, right, y, width, ui.selectedClipIds, 1);
      }
      for (const clip of [...track.text_clips, ...track.shape_clips]) {
        let start = clip.start_secs;
        let end = start + clip.duration_secs;
        if (drag && drag.mode !== "scrub" && drag.clipId === clip.id) {
          if (drag.mode === "move") {
            continue;
          } // drawn as a ghost below
          if (drag.mode === "trim-start") {
            start = Math.min(drag.ghostEdge, end - 0.1);
          }
          if (drag.mode === "trim-end") {
            end = Math.max(drag.ghostEdge, start + 0.1);
          }
        }
        const left = timeToX(start, px, scrollSecs);
        const right = timeToX(end, px, scrollSecs);
        if (right < 0 || left > width) {
          continue;
        }
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
      const clip = tracks
        .flatMap((t) => {
          return t.clips;
        })
        .find((c) => {
          return c.id === drag.clipId;
        });
      const overlay = clip
        ? undefined
        : tracks
            .flatMap((t) => {
              return [...t.text_clips, ...t.shape_clips];
            })
            .find((c) => {
              return c.id === drag.clipId;
            });
      if (clip && track) {
        const asset = media.find((a) => {
          return a.id === clip.asset_id;
        });
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
      const asset = media.find((a) => {
        return a.id === assetDrag.assetId;
      });
      if (asset && x >= 0 && x <= width && y >= RULER_HEIGHT) {
        const rowY = RULER_HEIGHT + Math.floor((y - RULER_HEIGHT) / TRACK_HEIGHT) * TRACK_HEIGHT;
        ctx.globalAlpha = 0.35;
        ctx.fillStyle = palette.accent;
        ctx.fillRect(x, rowY + 4, asset.duration_secs * px, TRACK_HEIGHT - 9);
        ctx.globalAlpha = 1;
      }
    }
    const effectDrag = effectDragStore.get();
    if (effectDrag) {
      const rect = canvas.getBoundingClientRect();
      const x = effectDrag.clientX - rect.left;
      const y = effectDrag.clientY - rect.top + scrollY;
      const target = effectTarget(tracks, x, y, px, scrollSecs);
      if (target) {
        const { span, trackIndex } = target;
        const left = timeToX(span.start_secs, px, scrollSecs);
        const right = timeToX(span.start_secs + span.duration_secs, px, scrollSecs);
        ctx.strokeStyle = palette.accent;
        ctx.lineWidth = 2;
        ctx.strokeRect(
          left + 1,
          RULER_HEIGHT + trackIndex * TRACK_HEIGHT + 3,
          right - left - 2,
          TRACK_HEIGHT - 7,
        );
        ctx.lineWidth = 1;
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
      ctx.fillText(rulerLabel(tick), x + 4, RULER_HEIGHT / 2);
    }
    for (const marker of markers) {
      const x = timeToX(marker.position_secs, px, scrollSecs);
      if (x < -8 || x > width + 8) {
        continue;
      }
      ctx.fillStyle = marker.completed ? palette.textMuted : palette.accent;
      ctx.beginPath();
      ctx.moveTo(x, 1);
      ctx.lineTo(x + 6, 1);
      ctx.lineTo(x, 8);
      ctx.fill();
    }

    // Playhead.
    // In/out marks (I and O): the range between them is shaded, each end gets a flag.
    const { markIn, markOut } = ui;
    if (markIn !== null || markOut !== null) {
      const inX = markIn !== null ? timeToX(markIn, px, scrollSecs) : null;
      const outX = markOut !== null ? timeToX(markOut, px, scrollSecs) : null;
      if (inX !== null && outX !== null && outX > inX) {
        ctx.fillStyle = "rgba(32, 185, 195, 0.14)";
        ctx.fillRect(inX, RULER_HEIGHT, outX - inX, height - RULER_HEIGHT);
      }
      ctx.fillStyle = palette.accent;
      if (inX !== null) {
        ctx.fillRect(Math.round(inX), 0, 2, height);
        ctx.fillRect(Math.round(inX), 0, 7, 3);
      }
      if (outX !== null) {
        ctx.fillRect(Math.round(outX) - 1, 0, 2, height);
        ctx.fillRect(Math.round(outX) - 6, 0, 7, 3);
      }
    }
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
  useLayoutEffect(() => {
    drawRef.current = draw;
  });
  const scheduleDraw = useCallback(() => {
    if (frameRef.current) {
      return;
    }
    frameRef.current = requestAnimationFrame(() => {
      frameRef.current = 0;
      drawRef.current();
    });
  }, []);

  // Canvas sizing follows the lanes container.
  useLayoutEffect(() => {
    const lanes = lanesRef.current;
    const canvas = canvasRef.current;
    if (!lanes || !canvas) {
      return;
    }
    const observer = new ResizeObserver(() => {
      const dpr = window.devicePixelRatio || 1;
      const width = lanes.clientWidth;
      const height = lanes.clientHeight;
      sizeRef.current = { width, height };
      setViewWidth(width);
      canvas.width = Math.round(width * dpr);
      canvas.height = Math.round(height * dpr);
      canvas.style.width = `${width}px`;
      canvas.style.height = `${height}px`;
      scheduleDraw();
    });
    observer.observe(lanes);
    return () => {
      return observer.disconnect();
    };
  }, [scheduleDraw]);

  useEffect(() => {
    window.addEventListener(MEDIA_EVENT, scheduleDraw);
    return () => {
      return window.removeEventListener(MEDIA_EVENT, scheduleDraw);
    };
  }, [scheduleDraw]);

  // Playhead ticks and library drags repaint without re-rendering React.
  useEffect(() => {
    const stopPreview = previewStore.subscribe(scheduleDraw);
    const stopAssetDrag = assetDragStore.subscribe(scheduleDraw);
    const stopEffectDrag = effectDragStore.subscribe(scheduleDraw);
    return () => {
      stopPreview();
      stopAssetDrag();
      stopEffectDrag();
      cancelAnimationFrame(frameRef.current);
      frameRef.current = 0;
    };
  }, [scheduleDraw]);

  useEffect(() => {
    return scheduleDraw();
  });

  // Library-to-timeline drops.
  useEffect(() => {
    const onUp = (e: PointerEvent) => {
      const drag = assetDragStore.get();
      const canvas = canvasRef.current;
      if (!drag || !canvas) {
        return;
      }
      const rect = canvas.getBoundingClientRect();
      const x = e.clientX - rect.left;
      const y = e.clientY - rect.top;
      if (x < 0 || y < RULER_HEIGHT || x > rect.width || y > rect.height) {
        return;
      }
      const { tracks, ui, scrollSecs, scrollY } = stateRef.current;
      const index = trackIndexAtY(y + scrollY, tracks.length);
      const start = Math.max(0, xToTime(x, ui.pxPerSec, scrollSecs));
      void run(api.addAssetToTimeline(drag.assetId, tracks[index]?.id, start));
    };
    window.addEventListener("pointerup", onUp);
    return () => {
      return window.removeEventListener("pointerup", onUp);
    };
  }, []);

  // Effect drops from the Effects tab: applied to the video clip under the pointer.
  useEffect(() => {
    const onUp = (e: PointerEvent) => {
      const drag = effectDragStore.get();
      const canvas = canvasRef.current;
      if (!drag || !canvas) {
        return;
      }
      const rect = canvas.getBoundingClientRect();
      const { tracks, ui, scrollSecs, scrollY } = stateRef.current;
      const target = effectTarget(
        tracks,
        e.clientX - rect.left,
        e.clientY - rect.top + scrollY,
        ui.pxPerSec,
        scrollSecs,
      );
      if (target) {
        patchUi({ selectedClipIds: [target.span.id] });
        void run(api.updateClip(target.span.id, drag.patch));
      }
    };
    window.addEventListener("pointerup", onUp);
    return () => {
      return window.removeEventListener("pointerup", onUp);
    };
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
    if (e.button !== 0) {
      return;
    }
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
          ? selected.filter((id) => {
              return id !== hit.clipId;
            })
          : [...selected, hit.clipId],
      });
      return;
    }
    if (!selected.includes(hit.clipId)) {
      patchUi({ selectedClipIds: [hit.clipId] });
    }
    const track = tracks[hit.trackIndex];
    if (track.locked) {
      return;
    }
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

  // Double-clicking a compound clip jumps the Editor to its nested sequence's tab -- the
  // common "enter" affordance every NLE with this feature has (ui::App::open_nested_sequence).
  const onDoubleClick = (e: React.MouseEvent<HTMLCanvasElement>) => {
    const { x, y } = lanePoint(e);
    if (y < RULER_HEIGHT) {
      return;
    }
    const hit = hitTest(tracks, x, y + scrollY, ui.pxPerSec, scrollSecs);
    if (!hit) {
      return;
    }
    const clip = tracks[hit.trackIndex].clips.find((c) => {
      return c.id === hit.clipId;
    });
    if (clip?.nested_sequence_id == null) {
      return;
    }
    const index = session.project.sequences.findIndex((s) => {
      return s.id === clip.nested_sequence_id;
    });
    if (index >= 0) {
      void run(api.selectSequence(index));
    }
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
    const candidates = ui.snapEnabled
      ? snapCandidates(tracks, drag.clipId, [previewStore.get().playheadSecs])
      : [];
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

  /** Abandons a drag in progress without committing it (Escape, or the system taking the pointer
   *  away): the clip stays where it was and the ghost disappears. */
  const cancelDrag = () => {
    if (dragRef.current) {
      dragRef.current = null;
      scheduleDraw();
    }
  };

  const onPointerUp = (e: React.PointerEvent<HTMLCanvasElement>) => {
    const drag = dragRef.current;
    dragRef.current = null;
    if (!drag) {
      return;
    }
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
      if (Math.abs(drag.deltaSecs) > 1e-6) {
        void run(api.slipClip(drag.clipId, drag.deltaSecs));
      }
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
      setScrollY((prev) => {
        return Math.max(0, Math.min(contentHeight - viewHeight, prev + e.deltaY));
      });
    } else {
      const delta = e.deltaX !== 0 ? e.deltaX : e.deltaY;
      setScrollSecs((prev) => {
        return Math.max(0, prev + delta / ui.pxPerSec);
      });
    }
  };

  /** Zooms out until the whole timeline fits the window, scrolled back to the start. */
  const zoomToFit = () => {
    const end = Math.max(
      0,
      ...tracks.flatMap((track) => {
        return [
          ...track.clips.map((clip) => {
            return clip.start_secs + clipDuration(clip);
          }),
          ...track.text_clips.map((clip) => {
            return clip.start_secs + clip.duration_secs;
          }),
          ...track.shape_clips.map((clip) => {
            return clip.start_secs + clip.duration_secs;
          }),
        ];
      }),
    );
    patchUi({
      pxPerSec: fitPxPerSec(end, sizeRef.current.width, MIN_PX_PER_SEC, MAX_PX_PER_SEC),
    });
    setScrollSecs(0);
  };

  const zoomBy = (factor: number) => {
    const anchor = sizeRef.current.width / 2;
    const zoom = zoomAround(
      anchor,
      ui.pxPerSec,
      scrollSecs,
      factor,
      MIN_PX_PER_SEC,
      MAX_PX_PER_SEC,
    );
    patchUi({ pxPerSec: zoom.pxPerSec });
    setScrollSecs(zoom.scrollSecs);
  };

  const actionRef = useRef<(action: TimelineAction) => void>(() => {
    return undefined;
  });
  useLayoutEffect(() => {
    actionRef.current = (action) => {
      if (action === "zoomIn") {
        zoomBy(1.5);
      } else if (action === "zoomOut") {
        zoomBy(1 / 1.5);
      } else if (action === "zoomFit") {
        zoomToFit();
      } else if (action === "scrollToPlayhead") {
        const visibleSecs = sizeRef.current.width / ui.pxPerSec;
        setScrollSecs(Math.max(0, previewStore.get().playheadSecs - visibleSecs / 2));
      }
    };
  });
  useEffect(() => {
    const onAction = (e: Event) => {
      actionRef.current((e as CustomEvent<TimelineAction>).detail);
    };
    window.addEventListener(TIMELINE_ACTION_EVENT, onAction);
    return () => {
      window.removeEventListener(TIMELINE_ACTION_EVENT, onAction);
    };
  }, []);

  const cmdItem = (id: string, overrides: Partial<ContextMenuItem> = {}): ContextMenuItem => {
    const command = findCommand(id);
    return {
      label: command ? t(command.label) : id,
      shortcut: shortcutLabel(id),
      disabled: command?.enabled ? !command.enabled() : false,
      onSelect: () => {
        void command?.run();
      },
      ...overrides,
    };
  };

  const onContextMenu = (e: React.MouseEvent<HTMLCanvasElement>) => {
    e.preventDefault();
    const { x, y } = lanePoint(e);
    if (y < RULER_HEIGHT) {
      const secs = Math.max(0, xToTime(x, ui.pxPerSec, scrollSecs));
      setMenu({
        x: e.clientX,
        y: e.clientY,
        items: [
          {
            label: t("addMarker"),
            shortcut: shortcutLabel("addMarker"),
            onSelect: () => {
              void run(api.seek(secs)).then(() => {
                return run(api.addMarker());
              });
            },
          },
          {
            label: t("cmdMarkIn"),
            shortcut: shortcutLabel("markIn"),
            onSelect: () => {
              patchUi({ markIn: secs });
            },
          },
          {
            label: t("cmdMarkOut"),
            shortcut: shortcutLabel("markOut"),
            onSelect: () => {
              patchUi({ markOut: secs });
            },
          },
          cmdItem("clearMarks", { separator: true }),
          cmdItem("deleteRange"),
          cmdItem("liftRange"),
          cmdItem("playRange"),
        ],
      });
      return;
    }
    const hit = hitTest(tracks, x, y + scrollY, ui.pxPerSec, scrollSecs);
    if (hit && !ui.selectedClipIds.includes(hit.clipId)) {
      patchUi({ selectedClipIds: [hit.clipId] });
    }
    setMenu({
      x: e.clientX,
      y: e.clientY,
      items: hit
        ? [
            cmdItem("split"),
            cmdItem("copy", { separator: true }),
            cmdItem("cut"),
            cmdItem("paste"),
            cmdItem("duplicate"),
            cmdItem("delete", { separator: true }),
            cmdItem("rippleDelete"),
          ]
        : [cmdItem("paste"), cmdItem("selectAll"), cmdItem("zoomFit", { separator: true })],
    });
  };

  const trackMenu = (track: Track, e: React.MouseEvent) => {
    e.preventDefault();
    const flag = (patch: Parameters<typeof api.setTrackFlags>[1]) => {
      return () => {
        void run(api.setTrackFlags(track.id, patch));
      };
    };
    const items: ContextMenuItem[] = [
      {
        label: track.visible ? t("hideTrack") : t("showTrack"),
        onSelect: flag({ visible: !track.visible }),
      },
      {
        label: track.locked ? t("unlockTrack") : t("lockTrack"),
        onSelect: flag({ locked: !track.locked }),
      },
    ];
    if (track.kind === "Video" || track.kind === "Audio") {
      items.push(
        {
          label: track.muted ? t("unmuteTrack") : t("muteTrack"),
          onSelect: flag({ muted: !track.muted }),
        },
        {
          label: track.solo ? t("unsoloTrack") : t("soloTrack"),
          onSelect: flag({ solo: !track.solo }),
        },
      );
    }
    for (const [kind, label] of [
      ["Video", "addVideoTrack"],
      ["Audio", "addAudioTrack"],
      ["Text", "addTextTrack"],
      ["Shape", "addShapeTrack"],
    ] as const) {
      items.push({
        label: t(label),
        separator: kind === "Video",
        onSelect: () => {
          void run(api.addTrack(kind));
        },
      });
    }
    setMenu({ x: e.clientX, y: e.clientY, items });
  };

  /** Clips in timeline order, for keyboard navigation and the screen-reader list. */
  const orderedClips = tracks
    .flatMap((track, row) => {
      return [
        ...track.clips.map((clip) => {
          return {
            id: clip.id,
            start: clip.start_secs,
            row,
            label:
              media.find((asset) => {
                return asset.id === clip.asset_id;
              })?.file_name ?? "#" + clip.asset_id,
            track: track.name,
          };
        }),
        ...track.text_clips.map((clip) => {
          return { id: clip.id, start: clip.start_secs, row, label: clip.text, track: track.name };
        }),
        ...track.shape_clips.map((clip) => {
          return { id: clip.id, start: clip.start_secs, row, label: t("shape"), track: track.name };
        }),
      ];
    })
    .sort((a, b) => {
      return a.start - b.start || a.row - b.row;
    });

  const onCanvasKeyDown = (e: React.KeyboardEvent<HTMLCanvasElement>) => {
    if (e.key === "Escape" && dragRef.current) {
      e.preventDefault();
      e.nativeEvent.stopPropagation();
      cancelDrag();
      return;
    }
    if (e.key !== "ArrowLeft" && e.key !== "ArrowRight" && e.key !== "Enter") {
      return;
    }
    if (orderedClips.length === 0 || e.ctrlKey || e.metaKey || e.altKey) {
      return;
    }
    e.preventDefault();
    // The window-level shortcuts (arrows step the playhead) must not also run.
    e.nativeEvent.stopPropagation();
    const current = orderedClips.findIndex((clip) => {
      return clip.id === ui.selectedClipIds[0];
    });
    if (e.key === "Enter") {
      if (current >= 0) {
        void run(api.seek(orderedClips[current].start));
      }
      return;
    }
    const next =
      current < 0
        ? 0
        : Math.min(
            orderedClips.length - 1,
            Math.max(0, current + (e.key === "ArrowRight" ? 1 : -1)),
          );
    patchUi({ selectedClipIds: [orderedClips[next].id] });
    void run(api.seek(orderedClips[next].start));
  };

  const selected = ui.selectedClipIds;
  return (
    <section
      className={cx(
        styles,
        baseStyles,
        chromeStyles,
        libraryStyles,
        editorStyles,
        "panel timeline",
      )}
    >
      <TimelineToolbar
        session={session}
        selected={selected}
        markers={markers}
        zoomBy={zoomBy}
        zoomToFit={zoomToFit}
      />
      <TimelineMinimap
        tracks={tracks}
        scrollSecs={scrollSecs}
        viewSecs={viewWidth / ui.pxPerSec}
        onScrollTo={setScrollSecs}
      />
      <div className={cx(styles, baseStyles, "timeline-body")}>
        <div
          className={cx(styles, baseStyles, "track-headers")}
          style={{ width: HEADER_WIDTH }}
        >
          <div style={{ height: RULER_HEIGHT }} />
          <div style={{ transform: `translateY(${-scrollY}px)` }}>
            {tracks.map((track) => {
              return (
                <TrackHeader
                  key={track.id}
                  track={track}
                  onContextMenu={(e) => {
                    trackMenu(track, e);
                  }}
                />
              );
            })}
          </div>
        </div>
        <div
          className={cx(styles, baseStyles, "timeline-lanes")}
          ref={lanesRef}
        >
          <canvas
            ref={canvasRef}
            data-testid="timeline-canvas"
            tabIndex={0}
            role="application"
            aria-label={t("timelineCanvas")}
            style={{ cursor }}
            onKeyDown={onCanvasKeyDown}
            onContextMenu={onContextMenu}
            onPointerDown={onPointerDown}
            onDoubleClick={onDoubleClick}
            onPointerMove={onPointerMove}
            onPointerUp={onPointerUp}
            onPointerCancel={cancelDrag}
            onWheel={onWheel}
          />
          <ul
            className={cx(styles, baseStyles, chromeStyles, "sr-only")}
            aria-label={t("timelineClips")}
          >
            {orderedClips.map((clip) => {
              return (
                <li key={clip.id}>
                  <button
                    aria-pressed={ui.selectedClipIds.includes(clip.id)}
                    onClick={() => {
                      patchUi({ selectedClipIds: [clip.id] });
                    }}
                  >
                    {clip.label}, {clip.track}, {formatTime(clip.start)}
                  </button>
                </li>
              );
            })}
          </ul>
          {tracks.length === 0 && (
            <div
              className={cx(styles, baseStyles, libraryStyles, "empty-state")}
              style={{ position: "absolute", top: RULER_HEIGHT + 16, width: "100%" }}
            >
              {t("timelineEmpty")}
            </div>
          )}
        </div>
      </div>
      {menu && (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          items={menu.items}
          onClose={() => {
            setMenu(null);
          }}
        />
      )}
    </section>
  );
}
