import { t } from "../../i18n";
import { frameAt, frameStepSecs, peakRange, peaksFor } from "./timelineMedia";
import { dissolveSpanSecs, TRACK_HEIGHT } from "../../timelineGeometry";
import {
  clipDuration,
  type ClipInstance,
  type MediaAsset,
  type ShapeClip,
  type TextClip,
  type Track,
} from "../../types";

export const UI_FONT = "11px 'Segoe UI', system-ui, sans-serif";
export const MONO_FONT = "11px 'Cascadia Mono', Consolas, monospace";

export interface Palette {
  bg: string;
  surface: string;
  surface2: string;
  border: string;
  text: string;
  textMuted: string;
  accent: string;
  video: string;
  videoEdge: string;
  audio: string;
  audioEdge: string;
  playhead: string;
}

export function readPalette(): Palette {
  const css = getComputedStyle(document.documentElement);
  const v = (name: string) => {
    return css.getPropertyValue(name).trim();
  };
  return {
    bg: v("--bg-workspace"),
    surface: v("--surface"),
    surface2: v("--surface-2"),
    border: v("--border-subtle"),
    text: v("--text-primary"),
    textMuted: v("--text-muted"),
    accent: v("--accent"),
    video: v("--media-video"),
    videoEdge: v("--media-video-edge"),
    audio: v("--audio-tint"),
    audioEdge: v("--audio-edge"),
    playhead: v("--playhead"),
  };
}

export function drawClip(
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
  const body = clip.color_label
    ? `rgb(${clip.color_label.join(",")})`
    : track.kind === "Audio"
      ? palette.audio
      : palette.video;
  ctx.save();
  ctx.beginPath();
  ctx.roundRect(left, top, width, height, 4);
  ctx.clip();
  ctx.fillStyle = body;
  ctx.fillRect(left, top, width, height);
  const sourceSpan = clip.source_out_secs - clip.source_in_secs;
  if (track.kind === "Video" && asset && sourceSpan > 0) {
    // Real frames along the clip: one tile per ~16:9 slot, each showing the source frame at
    // the tile's start, quantized so panning and zooming keep reusing the same frames.
    const tile = Math.max(32, Math.round((height * 16) / 9));
    const step = frameStepSecs((tile / width) * sourceSpan);
    const from = Math.max(left, 0);
    const to = Math.min(left + width, viewWidth);
    for (let x = left + Math.floor((from - left) / tile) * tile; x < to; x += tile) {
      const secs = clip.source_in_secs + (Math.max(0, x - left) / width) * sourceSpan;
      const frame = frameAt(asset.id, secs, step);
      if (frame) {
        ctx.drawImage(frame, x, top, tile, height);
      }
    }
    ctx.fillStyle = body;
    ctx.globalAlpha = alpha * 0.3;
    ctx.fillRect(left, top, width, height);
    ctx.globalAlpha = alpha;
  }

  if (asset && asset.has_audio) {
    drawWaveform(ctx, asset, clip, track.kind === "Audio", left, width, top, height, viewWidth);
  }
  // A cross-dissolve: the clip overlaps the one before it here, fading in over it.
  const dissolveWidth = (dissolveSpanSecs(track, clip) / clipDuration(clip)) * width;
  if (dissolveWidth > 1) {
    const fade = ctx.createLinearGradient(left, 0, left + dissolveWidth, 0);
    fade.addColorStop(0, "rgba(255,255,255,0.75)");
    fade.addColorStop(1, "rgba(255,255,255,0)");
    ctx.fillStyle = fade;
    ctx.beginPath();
    ctx.moveTo(left, top);
    ctx.lineTo(left + dissolveWidth, top);
    ctx.lineTo(left, top + height);
    ctx.closePath();
    ctx.fill();
    ctx.strokeStyle = "rgba(255,255,255,0.9)";
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.moveTo(left + dissolveWidth, top);
    ctx.lineTo(left, top + height);
    ctx.stroke();
  }
  ctx.fillStyle = "rgba(0,0,0,0.38)";
  ctx.fillRect(left, top, width, 16);
  ctx.restore();
  if (!clip.color_label) {
    ctx.strokeStyle = track.kind === "Audio" ? palette.audioEdge : palette.videoEdge;
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.roundRect(left + 0.5, top + 0.5, width - 1, height - 1, 4);
    ctx.stroke();
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
    const badges = [
      clip.speed_factor !== 1 ? `${clip.speed_factor.toFixed(2)}×` : "",
      clip.frozen ? "❄" : "",
    ]
      .filter(Boolean)
      .join(" ");
    ctx.fillText(badges ? `${label}  ${badges}` : label, Math.max(left, 0) + 5, top + 3);
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
export function drawOverlayClip(
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
const peak: [number, number] = [0, 0];
export function drawWaveform(
  ctx: CanvasRenderingContext2D,
  asset: MediaAsset,
  clip: ClipInstance,
  fullHeight: boolean,
  left: number,
  width: number,
  top: number,
  height: number,
  viewWidth: number,
) {
  const pyramid = peaksFor(asset);
  if (!pyramid || asset.duration_secs <= 0) {
    return;
  }
  const gain = Math.pow(10, clip.gain_db / 20);
  // Audio tracks use the whole clip body; on video clips the wave is a strip along the bottom.
  const bandTop = fullHeight ? top + 16 : top + height * 0.64;
  const bandHeight = fullHeight ? height - 16 : height * 0.36;
  const mid = bandTop + bandHeight / 2;
  const half = bandHeight / 2 - 1;
  const sourceSpan = clip.source_out_secs - clip.source_in_secs;
  const firstColumn = Math.max(0, Math.floor(-left));
  const lastColumn = Math.min(width, viewWidth - left);
  ctx.fillStyle = fullHeight ? "rgba(255,255,255,0.55)" : "rgba(255,255,255,0.7)";
  for (let column = firstColumn; column < lastColumn; column += 1) {
    const from = clip.source_in_secs + (column / width) * sourceSpan;
    const to = clip.source_in_secs + ((column + 1) / width) * sourceSpan;
    if (!peakRange(pyramid, from, to, peak)) {
      continue;
    }
    const high = Math.min(1, peak[1] * gain) * half;
    const low = Math.max(-1, peak[0] * gain) * half;
    ctx.fillRect(left + column, mid - high, 1, Math.max(1, high - low));
  }
}
