import { t } from "../i18n";
import { createStore, previewStore, useStore } from "../store";

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
  const channels = [
    { peak: tick.peakL, held: hold.left },
    { peak: tick.peakR, held: hold.right },
  ];
  return (
    <aside
      className="audio-meter"
      aria-label={t("audioMeter")}
    >
      <div
        className="meter-scale"
        aria-hidden="true"
      >
        {MARKS.map((mark) => {
          return <span key={mark}>{mark}</span>;
        })}
      </div>
      <div
        className="meter-bars"
        aria-hidden="true"
      >
        {channels.map(({ peak, held }, channel) => {
          const heldFill = peakFill(held);
          return (
            <div
              key={channel}
              className="meter-bar"
            >
              <div
                className="meter-cover"
                style={{ height: (1 - peakFill(peak)) * 100 + "%" }}
              />
              {held > 0 && (
                <div
                  className={"meter-hold" + (heldFill >= CLIP_FILL ? " clip" : "")}
                  style={{ bottom: heldFill * 100 + "%" }}
                />
              )}
            </div>
          );
        })}
      </div>
      <div
        className="meter-channels"
        aria-hidden="true"
      >
        <span>L</span>
        <span>R</span>
      </div>
    </aside>
  );
}
