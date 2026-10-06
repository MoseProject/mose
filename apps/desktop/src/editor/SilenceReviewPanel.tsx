import homeStyles from "../screens/home.module.css";
import { cx } from "../cx";
import baseStyles from "../styles/base.module.css";
import styles from "./chrome/chrome.module.css";
import { useState } from "react";
import { formatTime, t } from "../i18n";
import { api } from "../ipc";
import { Dialog } from "./chrome/Dialog";
import { Icon } from "./chrome/Icon";
import { patchUi, run, uiStore, useStore } from "../store";
import type { SilenceGap } from "../types";

interface StagedGap extends SilenceGap {
  accepted: boolean;
}

/** Silence-gap review popup -- D1 differentiator, mirrors `ui::App`'s `silence_review.rs`
 *  modal: scan the selected clip's track, list every gap with a checkbox defaulting to
 *  accepted, click-to-seek on the timecode, Apply ripple-deletes whatever's still checked. */
export function SilenceReviewPanel() {
  const ui = useStore(uiStore);
  const [trackId, setTrackId] = useState<number | null>(null);
  const [gaps, setGaps] = useState<StagedGap[]>([]);
  const open = trackId !== null;

  const begin = () => {
    const clipId = ui.selectedClipIds[0];
    if (clipId === undefined) {
      patchUi({ error: t("silenceReviewSelectClipFirst") });
      return;
    }
    void run(api.detectSilenceGapsForClip(clipId)).then((review) => {
      if (!review) {
        return;
      }
      setTrackId(review.trackId);
      setGaps(
        review.gaps.map((g) => {
          return { ...g, accepted: true };
        }),
      );
    });
  };

  const close = () => {
    return setTrackId(null);
  };

  const apply = () => {
    const accepted = gaps
      .filter((g) => {
        return g.accepted;
      })
      .map(({ startSecs, endSecs }) => {
        return { startSecs, endSecs };
      });
    if (trackId !== null && accepted.length > 0) {
      void run(api.applySilenceGaps(trackId, accepted));
    }
    close();
  };

  if (!open) {
    return (
      <button
        className={cx(styles, baseStyles, homeStyles, "icon")}
        onClick={begin}
      >
        {t("silenceReview")}
      </button>
    );
  }

  return (
    <Dialog
      title={t("silenceReviewTitle")}
      onClose={close}
    >
      {gaps.length === 0 ? (
        <p className={cx(styles, baseStyles, "muted")}>{t("silenceReviewEmpty")}</p>
      ) : (
        <div style={{ maxHeight: 320, overflowY: "auto" }}>
          {gaps.map((gap, index) => {
            return (
              <div
                className={cx(styles, baseStyles, "modal-row")}
                key={index}
              >
                <label className={cx(styles, baseStyles, "check")}>
                  <input
                    type="checkbox"
                    checked={gap.accepted}
                    onChange={() => {
                      const next = [...gaps];
                      next[index] = { ...gap, accepted: !gap.accepted };
                      setGaps(next);
                    }}
                  />
                  {`${formatTime(gap.startSecs)} - ${formatTime(gap.endSecs)} (${(gap.endSecs - gap.startSecs).toFixed(1)}s)`}
                </label>
                <button
                  className={cx(styles, baseStyles, "icon")}
                  aria-label={t("goToGap")}
                  title={t("goToGap")}
                  onClick={() => {
                    return void run(api.seek(gap.startSecs));
                  }}
                >
                  <Icon name="play" />
                </button>
              </div>
            );
          })}
        </div>
      )}
      <div className={cx(styles, baseStyles, "modal-actions")}>
        <button
          className={cx(styles, baseStyles, "icon")}
          onClick={close}
        >
          {t("cancel")}
        </button>
        <button
          className={cx(styles, baseStyles, "icon primary")}
          disabled={gaps.length === 0}
          onClick={apply}
        >
          {t("silenceReviewApply")}
        </button>
      </div>
    </Dialog>
  );
}
