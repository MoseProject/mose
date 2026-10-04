import { t } from "../i18n";
import { previewStore, useStore } from "../store";

/** Scale marks, top to bottom, in dBFS. */
const MARKS = [0, -6, -12, -18, -24, -30, -36, -42, -48, -54, -60];
const FLOOR_DB = -60;

/** Linear peak to a 0..1 fill of the -60..0 dBFS scale. */
export function peakFill(peak: number): number {
  if (!(peak > 0)) return 0;
  const db = 20 * Math.log10(peak);
  return Math.min(1, Math.max(0, (db - FLOOR_DB) / -FLOOR_DB));
}

/** Vertical stereo level meter beside the inspector, driven by the preview's peak readout. */
export function AudioMeter() {
  const tick = useStore(previewStore);
  return (
    <aside className="audio-meter" aria-label={t("audioMeter")}>
      <div className="meter-scale" aria-hidden="true">
        {MARKS.map((mark) => (
          <span key={mark}>{mark}</span>
        ))}
      </div>
      <div className="meter-bars" aria-hidden="true">
        {[tick.peakL, tick.peakR].map((peak, channel) => (
          <div key={channel} className="meter-bar">
            <div className="meter-cover" style={{ height: `${(1 - peakFill(peak)) * 100}%` }} />
          </div>
        ))}
      </div>
      <div className="meter-channels" aria-hidden="true">
        <span>L</span>
        <span>R</span>
      </div>
    </aside>
  );
}
