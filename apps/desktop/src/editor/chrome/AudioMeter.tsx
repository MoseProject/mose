import { cx } from "../../cx";
import baseStyles from "../../styles/base.module.css";
import styles from "./audio-meter.module.css";
import { t } from "../../i18n";
import { createStore, previewStore, sessionStore, useStore } from "../../store";

/** Scale marks, top to bottom, in dBFS. */
const MARKS = [0, -6, -12, -18, -24, -30, -36, -42, -48, -54, -60];
const FLOOR_DB = -60;
/** How long a peak marker stays before the next, lower one may replace it. */
const HOLD_MS = 1500;
/** At or above this fill (about -1.2 dBFS) the channel is treated as clipping. */
const CLIP_FILL = 0.98;

/** Linear peak to a 0..1 fill of the -60..0 dBFS scale. */
export function peakFill(peak: number): number {
  if (!(peak > 0)) {
    return 0;
  }
  const db = 20 * Math.log10(peak);
  return Math.min(1, Math.max(0, (db - FLOOR_DB) / -FLOOR_DB));
}

/** Loudness targets marked on the scale, in LUFS: the platform presets the export offers. */
const TARGETS = [-14, -16, -23];
/** How far from the sequence target the reading may drift before it is flagged, in LU. */
const TOLERANCE_LU = 1;

/** Where a LUFS value sits on the dBFS scale, as 0..1 from the bottom. */
export function scaleFill(db: number): number {
  return Math.min(1, Math.max(0, (db - FLOOR_DB) / -FLOOR_DB));
}

/** "ok" within a tolerance of the target, "hot" above it, "low" below it. */
export function loudnessState(lufs: number, target: number): "ok" | "hot" | "low" {
  if (lufs > target + TOLERANCE_LU) {
    return "hot";
  }
  return lufs < target - TOLERANCE_LU ? "low" : "ok";
}

interface Hold {
  left: number;
  right: number;
  leftAt: number;
  rightAt: number;
}

/** Peak-hold markers, kept outside React and fed by the preview ticks. */
export const holdStore = createStore<Hold>({ left: 0, right: 0, leftAt: 0, rightAt: 0 });

/** Folds one tick's peaks into the held ones: a higher peak replaces the held value, a held
 *  value older than the hold time is replaced by whatever came in. Pure, for testing. */
export function nextHold(prev: Hold, peakLeft: number, peakRight: number, now: number): Hold {
  const keep = (held: number, at: number, peak: number) => {
    const expired = now - at > HOLD_MS;
    return peak >= held || expired ? { value: peak, at: now } : { value: held, at };
  };
  const left = keep(prev.left, prev.leftAt, peakLeft);
  const right = keep(prev.right, prev.rightAt, peakRight);
  return { left: left.value, leftAt: left.at, right: right.value, rightAt: right.at };
}

previewStore.subscribe(() => {
  const tick = previewStore.get();
  holdStore.set((prev) => {
    return nextHold(prev, tick.peakL, tick.peakR, performance.now());
  });
});

/** Vertical stereo level meter beside the inspector, driven by the preview's peak readout, with
 *  a peak-hold marker per channel that turns red when the signal reaches full scale. */
export function AudioMeter() {
  const tick = useStore(previewStore);
  const hold = useStore(holdStore);
  const session = useStore(sessionStore);
  const target = session
    ? session.project.sequences[session.project.active_sequence].export_settings.target_lufs
    : -14;
  const lufs = tick.momentaryLufs;
  const channels = [
    { peak: tick.peakL, held: hold.left },
    { peak: tick.peakR, held: hold.right },
  ];
  return (
    <aside
      className={cx(styles, baseStyles, "audio-meter")}
      aria-label={t("audioMeter")}
    >
      <div
        className={cx(styles, baseStyles, "meter-scale")}
        aria-hidden="true"
      >
        {MARKS.map((mark) => {
          return <span key={mark}>{mark}</span>;
        })}
      </div>
      <div
        className={cx(styles, baseStyles, "meter-bars")}
        aria-hidden="true"
      >
        {TARGETS.map((mark) => {
          return (
            <div
              key={mark}
              className={cx(
                styles,
                baseStyles,
                "meter-target" + (mark === target ? " active" : ""),
              )}
              style={{ bottom: scaleFill(mark) * 100 + "%" }}
              data-testid="meter-target"
            />
          );
        })}
        {channels.map(({ peak, held }, channel) => {
          const heldFill = peakFill(held);
          return (
            <div
              key={channel}
              className={cx(styles, baseStyles, "meter-bar")}
            >
              <div
                className={cx(styles, baseStyles, "meter-cover")}
                style={{ height: (1 - peakFill(peak)) * 100 + "%" }}
              />
              {held > 0 && (
                <div
                  className={cx(
                    styles,
                    baseStyles,
                    "meter-hold" + (heldFill >= CLIP_FILL ? " clip" : ""),
                  )}
                  style={{ bottom: heldFill * 100 + "%" }}
                />
              )}
            </div>
          );
        })}
      </div>
      <div
        className={cx(
          styles,
          baseStyles,
          "meter-lufs " + (lufs === null ? "idle" : loudnessState(lufs, target)),
        )}
        title={t("meterLoudness") + " (" + t("meterTarget") + " " + target + " LUFS)"}
        data-testid="meter-lufs"
      >
        {lufs === null ? "-" : lufs.toFixed(1)}
        <small>LUFS</small>
      </div>
      <div
        className={cx(styles, baseStyles, "meter-channels")}
        aria-hidden="true"
      >
        <span>L</span>
        <span>R</span>
      </div>
    </aside>
  );
}
