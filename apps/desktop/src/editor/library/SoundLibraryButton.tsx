import homeStyles from "../../screens/home.module.css";
import { cx } from "../../cx";
import baseStyles from "../../styles/base.module.css";
import styles from "./library.module.css";
import { useState } from "react";
import { formatTime, t } from "../../i18n";
import { api } from "../../ipc";
import { run } from "../../store";
import type { LibraryTrack } from "../../types";
import { Dialog } from "../chrome/Dialog";

export function SoundLibraryButton() {
  const [open, setOpen] = useState(false);
  const [tracks, setTracks] = useState<LibraryTrack[] | null>(null);

  const toggle = () => {
    if (!open) {
      void run(api.listSoundLibrary()).then((result) => {
        return setTracks(result ?? []);
      });
    }
    setOpen(!open);
  };

  return (
    <>
      <button
        className={cx(styles, baseStyles, homeStyles, "icon")}
        onClick={toggle}
      >
        {t("soundLibrary")}
      </button>
      {open && (
        <Dialog
          title={t("soundLibrary")}
          onClose={() => {
            setOpen(false);
          }}
        >
          {!tracks || tracks.length === 0 ? (
            <p className={cx(styles, baseStyles, "muted")}>{t("soundLibraryEmpty")}</p>
          ) : (
            <div style={{ maxHeight: 320, overflowY: "auto" }}>
              {tracks.map((track) => {
                return (
                  <div
                    className={cx(styles, baseStyles, "modal-row")}
                    key={track.path}
                  >
                    <span>
                      {track.category === "music" ? "🎵" : "🔊"} {track.name} (
                      {formatTime(track.durationSecs)})
                    </span>
                    <button
                      className={cx(styles, baseStyles, "icon")}
                      onClick={() => {
                        void run(api.importMedia([track.path]));
                        setOpen(false);
                      }}
                    >
                      {t("importFiles")}
                    </button>
                  </div>
                );
              })}
            </div>
          )}
        </Dialog>
      )}
    </>
  );
}
