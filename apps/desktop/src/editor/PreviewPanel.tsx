import type { ReactNode } from "react";
import { formatTime, t } from "../i18n";
import { api, frameUrl } from "../ipc";
import { previewStore, run, sessionStore, useStore } from "../store";
import { activeTimeline, clipDuration } from "../types";

export function PreviewPanel() {
  const tick = useStore(previewStore);
  const session = useStore(sessionStore);
  const duration = session
    ? Math.max(
        0,
        ...activeTimeline(session.project).tracks.flatMap((track) =>
          track.clips.map((clip) => clip.start_secs + clipDuration(clip)),
        ),
      )
    : 0;

  // The backend keeps the last frame while a new pipeline opens at a cut, so keep showing it
  // instead of flashing the placeholder; it clears the frame itself when nothing is there.
  let body: ReactNode = null;
  if (tick.hasClip && tick.error) {
    body = (
      <span className="muted" title={tick.error}>
        {t("previewUnavailable")}
      </span>
    );
  } else if (tick.hasClip && tick.frameSeq > 0) {
    body = <img src={frameUrl(tick.frameSeq)} alt="" draggable={false} />;
  } else if (!tick.hasClip) {
    body = <span className="muted">{t("previewEmpty")}</span>;
  }

  // Square-root the linear peak so quiet speech still visibly moves the bar.
  const meterWidth = (peak: number) => `${Math.min(1, Math.sqrt(Math.max(0, peak))) * 100}%`;

  return (
    <section className="panel preview">
      <div className="monitor">{body}</div>
      <div className="transport">
        <button className="icon" title={t("stepBack")} onClick={() => run(api.step(-1))}>
          ⏮
        </button>
        <button
          className="icon"
          title={tick.playing ? t("pause") : t("play")}
          onClick={() => run(api.setPlaying(!tick.playing))}
        >
          {tick.playing ? "⏸" : "▶"}
        </button>
        <button className="icon" title={t("stepForward")} onClick={() => run(api.step(1))}>
          ⏭
        </button>
        <span className="mono">
          {formatTime(tick.playheadSecs)} <span className="muted">/ {formatTime(duration)}</span>
        </span>
        <span style={{ flex: 1 }} />
        <div className="meter" aria-hidden>
          <div style={{ width: meterWidth(tick.peakL) }} />
          <div style={{ width: meterWidth(tick.peakR) }} />
        </div>
      </div>
    </section>
  );
}
