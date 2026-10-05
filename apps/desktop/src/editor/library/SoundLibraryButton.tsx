import { useState } from "react";
import { formatTime, t } from "../../i18n";
import { api } from "../../ipc";
import { run } from "../../store";
import type { LibraryTrack } from "../../types";
import { Dialog } from "../Dialog";

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
        className="icon"
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
            <p className="muted">{t("soundLibraryEmpty")}</p>
          ) : (
            <div style={{ maxHeight: 320, overflowY: "auto" }}>
              {tracks.map((track) => {
                return (
                  <div
                    className="modal-row"
                    key={track.path}
                  >
                    <span>
                      {track.category === "music" ? "🎵" : "🔊"} {track.name} (
                      {formatTime(track.durationSecs)})
                    </span>
                    <button
                      className="icon"
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
