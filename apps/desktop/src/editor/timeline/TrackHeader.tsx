import { cx } from "../../cx";
import baseStyles from "../../styles/base.module.css";
import styles from "./timeline.module.css";
import { t, type TextKey } from "../../i18n";
import { api } from "../../ipc";
import { Icon } from "../chrome/Icon";
import { run } from "../../store";
import { TRACK_HEIGHT } from "../../timelineGeometry";
import { memo } from "react";
import { TrackMixButton } from "./TrackMixButton";
import { type AudioRole, type Track } from "../../types";

const AUDIO_ROLES: [AudioRole, TextKey][] = [
  ["Unspecified", "roleUnspecified"],
  ["GameAudio", "roleGame"],
  ["Mic", "roleMic"],
  ["Music", "roleMusic"],
];

/** Memoized: zooming or scrolling re-renders the timeline many times a second, and a project can
 *  have dozens of tracks whose headers have not changed. */
export const TrackHeader = memo(function TrackHeader({
  track,
  onContextMenu,
}: {
  track: Track;
  onContextMenu: (track: Track, e: React.MouseEvent) => void;
}) {
  const carriesAudio = track.kind === "Video" || track.kind === "Audio";
  return (
    <div
      className={cx(styles, baseStyles, "track-header")}
      style={{ height: TRACK_HEIGHT }}
      onContextMenu={(e) => {
        onContextMenu(track, e);
      }}
    >
      <div className={cx(styles, baseStyles, "track-header-row")}>
        <span className={cx(styles, baseStyles, "name")}>{track.name}</span>
        <button
          className={cx(styles, baseStyles, `icon ${track.visible ? "" : "active"}`)}
          title={track.visible ? t("hideTrack") : t("showTrack")}
          aria-label={track.visible ? t("hideTrack") : t("showTrack")}
          aria-pressed={!track.visible}
          onClick={() => {
            return run(api.setTrackFlags(track.id, { visible: !track.visible }));
          }}
        >
          <Icon name={track.visible ? "eye" : "eyeOff"} />
        </button>
        <button
          className={cx(styles, baseStyles, `icon ${track.locked ? "active" : ""}`)}
          title={track.locked ? t("unlockTrack") : t("lockTrack")}
          aria-label={track.locked ? t("unlockTrack") : t("lockTrack")}
          aria-pressed={track.locked}
          onClick={() => {
            return run(api.setTrackFlags(track.id, { locked: !track.locked }));
          }}
        >
          <Icon name={track.locked ? "lock" : "unlock"} />
        </button>
        {carriesAudio && <TrackMixButton track={track} />}
      </div>
      {carriesAudio && (
        <div className={cx(styles, baseStyles, "track-header-row")}>
          <button
            className={cx(styles, baseStyles, `icon letter ${track.muted ? "active muted" : ""}`)}
            title={track.muted ? t("unmuteTrack") : t("muteTrack")}
            aria-label={track.muted ? t("unmuteTrack") : t("muteTrack")}
            aria-pressed={track.muted}
            onClick={() => {
              return run(api.setTrackFlags(track.id, { muted: !track.muted }));
            }}
          >
            M
          </button>
          <button
            className={cx(styles, baseStyles, `icon letter ${track.solo ? "active solo" : ""}`)}
            title={track.solo ? t("unsoloTrack") : t("soloTrack")}
            aria-label={track.solo ? t("unsoloTrack") : t("soloTrack")}
            aria-pressed={track.solo}
            onClick={() => {
              return run(api.setTrackFlags(track.id, { solo: !track.solo }));
            }}
          >
            S
          </button>
          {track.kind === "Audio" && (
            <select
              className={cx(styles, baseStyles, "role-select")}
              aria-label={t("audioRoleLabel")}
              title={t("audioRoleLabel")}
              value={track.audio_role}
              onChange={(e) => {
                return run(api.setTrackFlags(track.id, { audioRole: e.target.value as AudioRole }));
              }}
            >
              {AUDIO_ROLES.map(([role, label]) => {
                return (
                  <option
                    key={role}
                    value={role}
                  >
                    {t(label)}
                  </option>
                );
              })}
            </select>
          )}
        </div>
      )}
    </div>
  );
});
