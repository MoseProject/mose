import type { ExportCanvas, ExportOptions } from "../../types";

export type QualityChoice = "source" | "high" | "medium" | "low" | "custom";

/** Video bitrate, in Mbps, each fixed quality step asks for. */
export const QUALITY_MBPS: Record<Exclude<QualityChoice, "source" | "custom">, number> = {
  high: 16,
  medium: 8,
  low: 4,
};
/** Picture heights offered besides the original one; those above it scale the picture up. */
export const RESOLUTION_HEIGHTS = [2160, 1440, 1080, 720, 480];
export const MIN_MBPS = 0.5;
export const MAX_MBPS = 200;
/** The stereo AAC track every export carries, in Mbps. */
const AUDIO_MBPS = 0.192;

/** Whether `maxHeight` asks for a larger picture than the sequence's. */
export function isUpscale(canvas: ExportCanvas | null, maxHeight: number | null): boolean {
  return canvas !== null && maxHeight !== null && maxHeight > canvas.height;
}

export function customBitrateValid(mbps: number): boolean {
  return Number.isFinite(mbps) && mbps >= MIN_MBPS && mbps <= MAX_MBPS;
}

/** The options the backend takes for a quality step and a height ceiling (`null` is "original"). */
export function exportOptions(
  quality: QualityChoice,
  customMbps: number,
  maxHeight: number | null,
): ExportOptions {
  const options: ExportOptions = {};
  if (quality === "custom") {
    options.bitrateMbps = customMbps;
  } else if (quality !== "source") {
    options.bitrateMbps = QUALITY_MBPS[quality];
  }
  if (maxHeight !== null) {
    options.maxHeight = maxHeight;
  }
  return options;
}

export interface ExportSummary {
  width: number;
  height: number;
  bitrateMbps: number;
  sizeMb: number;
}

/** What the export will produce: the same arithmetic the backend applies (width kept even, source bitrate scaled with the pixel count unless one was asked
 *  for), and a size from bitrate times length. */
export function summarize(canvas: ExportCanvas, options: ExportOptions): ExportSummary {
  let { width, height, bitrateMbps } = canvas;
  if (options.maxHeight !== undefined && options.maxHeight !== canvas.height) {
    width = Math.max(2, Math.floor((canvas.width * options.maxHeight) / canvas.height / 2) * 2);
    height = Math.floor(options.maxHeight / 2) * 2;
    bitrateMbps = (canvas.bitrateMbps * (width * height)) / (canvas.width * canvas.height);
  }
  if (options.bitrateMbps !== undefined) {
    bitrateMbps = options.bitrateMbps;
  }
  const sizeMb = ((bitrateMbps + AUDIO_MBPS) * canvas.durationSecs) / 8;
  return { width, height, bitrateMbps, sizeMb };
}

/** "850 MB" / "1.2 GB". */
export function formatSize(mb: number): string {
  return mb >= 1024 ? `${(mb / 1024).toFixed(1)} GB` : `${Math.max(1, Math.round(mb))} MB`;
}
