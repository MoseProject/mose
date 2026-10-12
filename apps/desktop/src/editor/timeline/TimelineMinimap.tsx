import { cx } from "../../cx";
import baseStyles from "../../styles/base.module.css";
import styles from "./timeline.module.css";
import { useMemo, useRef } from "react";
import { t } from "../../i18n";
import { clipDuration, type Track } from "../../types";

const ROWS = 6;
/** Past this many clips the overview draws runs of touching clips instead of every one: at that
 *  size they are thinner than a pixel anyway, and a DOM element per clip made every scroll and
 *  zoom rebuild thousands of them. */
const MERGE_ABOVE = 300;

interface Span {
  start: number;
  length: number;
  kind: string;
  row: number;
}

/** Spans of one row and kind that overlap or touch, joined into one. */
function mergeSpans(spans: Span[], gap: number): Span[] {
  const sorted = [...spans].sort((a, b) => {
    return a.row - b.row || a.kind.localeCompare(b.kind) || a.start - b.start;
  });
  const merged: Span[] = [];
  for (const span of sorted) {
    const last = merged[merged.length - 1];
    if (
      last &&
      last.row === span.row &&
      last.kind === span.kind &&
      span.start <= last.start + last.length + gap
    ) {
      last.length = Math.max(last.length, span.start + span.length - last.start);
    } else {
      merged.push({ ...span });
    }
  }
  return merged;
}

/** A thin overview of the whole sequence with the visible window drawn over it: click or drag to
 *  move the timeline there. Lets a two-hour project be crossed in one gesture. */
export function TimelineMinimap({
  tracks,
  scrollSecs,
  viewSecs,
  onScrollTo,
}: {
  tracks: Track[];
  scrollSecs: number;
  viewSecs: number;
  onScrollTo: (secs: number) => void;
}) {
  const bar = useRef<HTMLDivElement>(null);
  const spans = useMemo(() => {
    return tracks.flatMap((track, row): Span[] => {
      const clips = track.clips.map((clip) => {
        return { start: clip.start_secs, length: clipDuration(clip), kind: track.kind, row };
      });
      const overlays = [...track.text_clips, ...track.shape_clips].map((clip) => {
        return { start: clip.start_secs, length: clip.duration_secs, kind: track.kind, row };
      });
      return [...clips, ...overlays];
    });
  }, [tracks]);
  const contentEnd = useMemo(() => {
    return spans.reduce((end, span) => {
      return Math.max(end, span.start + span.length);
    }, 0);
  }, [spans]);
  const total = Math.max(contentEnd, scrollSecs + viewSecs, 1);

  // The clips' marks depend on the sequence and its length, not on where the window is, so a
  // scroll leaves them alone.
  const marks = useMemo(() => {
    // One screen pixel of a typical overview is about a thousandth of its length.
    const drawn = spans.length > MERGE_ABOVE ? mergeSpans(spans, total / 1000) : spans;
    return drawn.map((span, index) => {
      return (
        <span
          key={index}
          className={cx(styles, baseStyles, `minimap-clip ${span.kind.toLowerCase()}`)}
          style={{
            left: `${(span.start / total) * 100}%`,
            width: `${Math.max(0.15, (span.length / total) * 100)}%`,
            top: `${(Math.min(span.row, ROWS - 1) / ROWS) * 100}%`,
            height: `${100 / ROWS}%`,
          }}
        />
      );
    });
  }, [spans, total]);

  const moveTo = (clientX: number) => {
    const box = bar.current?.getBoundingClientRect();
    if (!box || box.width === 0) {
      return;
    }
    const ratio = Math.min(1, Math.max(0, (clientX - box.left) / box.width));
    onScrollTo(Math.max(0, ratio * total - viewSecs / 2));
  };

  return (
    <div
      ref={bar}
      className={cx(styles, baseStyles, "minimap")}
      role="scrollbar"
      aria-orientation="horizontal"
      aria-label={t("timelineOverview")}
      aria-valuemin={0}
      aria-valuemax={Math.round(total)}
      aria-valuenow={Math.round(scrollSecs)}
      onPointerDown={(e) => {
        e.currentTarget.setPointerCapture(e.pointerId);
        moveTo(e.clientX);
      }}
      onPointerMove={(e) => {
        if (e.currentTarget.hasPointerCapture(e.pointerId)) {
          moveTo(e.clientX);
        }
      }}
    >
      {marks}
      <span
        className={cx(styles, baseStyles, "minimap-window")}
        style={{
          left: `${(scrollSecs / total) * 100}%`,
          width: `${Math.min(100, (viewSecs / total) * 100)}%`,
        }}
      />
    </div>
  );
}
