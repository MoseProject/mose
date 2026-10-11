import { cx } from "../cx";
import baseStyles from "../styles/base.module.css";
import styles from "./topbar.module.css";
import { formatTimecode, t } from "../i18n";
import { previewStore, useStore } from "../store";
import type { SessionView } from "../types";

/** Playhead timecode (hours:minutes:seconds:frames) plus the source's frame rate and
 *  resolution, read-only. */
export function Readout({ session }: { session: SessionView }) {
  const tick = useStore(previewStore);
  const source = session.project.media_library.find((asset) => {
    return asset.kind === "Video" && asset.resolution;
  });
  return (
    <div
      className={cx(styles, baseStyles, "readout")}
      aria-label={t("timecode")}
    >
      <span className={cx(styles, baseStyles, "timecode mono")}>
        {formatTimecode(tick.playheadSecs, source?.fps ?? 30)}
      </span>
      {source?.fps != null && (
        <span className={cx(styles, baseStyles, "muted mono")}>{Math.round(source.fps)} FPS</span>
      )}
      {source?.resolution && (
        <span className={cx(styles, baseStyles, "muted mono")}>
          {source.resolution[0]} × {source.resolution[1]}
        </span>
      )}
    </div>
  );
}
