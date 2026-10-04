import { useState } from "react";
import { formatTime, t } from "../i18n";
import { api } from "../ipc";
import { Icon } from "./Icon";
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
        className="icon"
        onClick={begin}
      >
        {t("silenceReview")}
      </button>
    );
  }

  return (
    <div
      className="modal-overlay"
      onClick={close}
    >
      <div
        className="modal"
        onClick={(e) => {
          return e.stopPropagation();
        }}
      >
        <h3>{t("silenceReviewTitle")}</h3>
        {gaps.length === 0 ? (
          <p className="muted">{t("silenceReviewEmpty")}</p>
        ) : (
          <div style={{ maxHeight: 320, overflowY: "auto" }}>
            {gaps.map((gap, index) => {
              return (
                <div
                  className="modal-row"
                  key={index}
                >
                  <label className="check">
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
                    className="icon"
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
        <div className="modal-actions">
          <button
            className="icon"
            onClick={close}
          >
            {t("cancel")}
          </button>
          <button
            className="icon primary"
            disabled={gaps.length === 0}
            onClick={apply}
          >
            {t("silenceReviewApply")}
          </button>
        </div>
      </div>
    </div>
  );
}
