import homeStyles from "../../screens/home.module.css";
import { cx } from "../../cx";
import baseStyles from "../../styles/base.module.css";
import styles from "./library.module.css";
import { useState } from "react";
import { t } from "../../i18n";
import { api, onYoutubeDownload } from "../../ipc";
import { run } from "../../store";
import { Dialog } from "../chrome/Dialog";

export function YoutubeDownloadButton() {
  const [open, setOpen] = useState(false);
  const [url, setUrl] = useState("");
  const [format, setFormat] = useState<"mp4" | "mp3">("mp4");
  const [quality, setQuality] = useState("best");
  const [downloading, setDownloading] = useState(false);
  const [progress, setProgress] = useState(0);
  const [error, setError] = useState<string | null>(null);

  const openModal = () => {
    setOpen(true);
    setError(null);
    void run(api.isYtDlpAvailable()).then((available) => {
      if (available === false) {
        setError(t("youtubeDownloadToolMissing"));
      }
    });
  };

  const start = () => {
    if (!url.trim()) {
      return;
    }
    setDownloading(true);
    setProgress(0);
    setError(null);
    const unlisten = onYoutubeDownload((event) => {
      if (event.type === "progress") {
        setProgress(event.fraction);
      }
      if (event.type === "done") {
        setDownloading(false);
        setOpen(false);
        setUrl("");
        void unlisten.then((stop) => {
          return stop();
        });
      }
      if (event.type === "failed") {
        setDownloading(false);
        setError(event.message);
        void unlisten.then((stop) => {
          return stop();
        });
      }
    });
    // `run` swallows the rejection and resolves undefined, so success is marked explicitly.
    void run(
      api.downloadYoutube(url, format, quality).then(() => {
        return true;
      }),
    ).then((started) => {
      if (started) {
        return;
      }
      setDownloading(false);
      void unlisten.then((stop) => {
        return stop();
      });
    });
  };

  if (!open) {
    return (
      <button
        className={cx(styles, baseStyles, homeStyles, "icon")}
        onClick={openModal}
      >
        {t("youtubeDownload")}
      </button>
    );
  }

  return (
    <Dialog
      title={t("youtubeDownload")}
      onClose={() => {
        if (!downloading) {
          setOpen(false);
        }
      }}
    >
      {error && <p style={{ color: "var(--error, #e55)" }}>{error}</p>}
      <input
        autoFocus
        disabled={downloading}
        placeholder={t("youtubeDownloadUrlHint")}
        value={url}
        onChange={(e) => {
          return setUrl(e.target.value);
        }}
      />
      <div className={cx(styles, baseStyles, "modal-row")}>
        <select
          disabled={downloading}
          value={format}
          onChange={(e) => {
            setFormat(e.target.value as "mp4" | "mp3");
            setQuality(e.target.value === "mp4" ? "best" : "192");
          }}
        >
          <option value="mp4">MP4</option>
          <option value="mp3">MP3</option>
        </select>
        <select
          disabled={downloading}
          value={quality}
          onChange={(e) => {
            return setQuality(e.target.value);
          }}
        >
          {format === "mp4"
            ? ["best", "1080", "720", "480", "360"].map((q) => {
                return (
                  <option
                    key={q}
                    value={q}
                  >
                    {q === "best" ? "Best" : `${q}p`}
                  </option>
                );
              })
            : ["320", "192", "128"].map((q) => {
                return (
                  <option
                    key={q}
                    value={q}
                  >
                    {q} kbps
                  </option>
                );
              })}
        </select>
      </div>
      {downloading && (
        <progress
          value={progress}
          max={1}
          style={{ width: "100%" }}
        />
      )}
      <div className={cx(styles, baseStyles, "modal-actions")}>
        {downloading ? (
          <button
            className={cx(styles, baseStyles, "icon")}
            onClick={() => {
              return void run(api.cancelYoutubeDownload());
            }}
          >
            {t("youtubeDownloadCancel")}
          </button>
        ) : (
          <>
            <button
              className={cx(styles, baseStyles, "icon")}
              onClick={() => {
                return setOpen(false);
              }}
            >
              {t("cancel")}
            </button>
            <button
              className={cx(styles, baseStyles, "icon primary")}
              disabled={url.trim().length === 0}
              onClick={start}
            >
              {t("youtubeDownloadStart")}
            </button>
          </>
        )}
      </div>
    </Dialog>
  );
}
