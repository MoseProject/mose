import { useEffect, useState } from "react";
import { t } from "../i18n";
import { api } from "../ipc";
import { Dialog } from "../editor/Dialog";
import { run } from "../store";
import type { CrashReview } from "../types";

/** Post-crash "Send once / Always send / Do not send" review -- mirrors `ui::App`'s
 *  `crash_review.rs` modal. Checked once at startup; renders nothing once dismissed. */
export function CrashReviewModal() {
  const [crash, setCrash] = useState<CrashReview | null>(null);
  const [showPayload, setShowPayload] = useState(false);

  useEffect(() => {
    void run(api.getPendingCrashReview()).then((result) => {
      return setCrash(result ?? null);
    });
  }, []);

  if (!crash) {
    return null;
  }

  const finish = (action: () => Promise<unknown>) => {
    setCrash(null);
    void run(action());
  };

  return (
    <Dialog
      title={t("crashReviewTitle")}
      dismissible={false}
      onClose={() => {
        return undefined;
      }}
    >
      <p>{t("crashReviewOffer")}</p>
      <button
        className="icon"
        onClick={() => {
          return setShowPayload((v) => {
            return !v;
          });
        }}
      >
        {t("crashReviewShowPayload")}
      </button>
      {showPayload && (
        <pre
          style={{
            maxHeight: 220,
            overflow: "auto",
            fontSize: 11,
            whiteSpace: "pre-wrap",
          }}
        >
          {crash.payloadPreview}
        </pre>
      )}
      <div className="modal-actions">
        <button
          className="icon"
          onClick={() => {
            return finish(() => {
              return api.sendCrashOnce();
            });
          }}
        >
          {t("crashReviewSendOnce")}
        </button>
        <button
          className="icon"
          onClick={() => {
            return finish(() => {
              return api.alwaysSendCrash();
            });
          }}
        >
          {t("crashReviewAlwaysSend")}
        </button>
        <button
          className="icon"
          onClick={() => {
            return finish(() => {
              return api.dismissCrashReview();
            });
          }}
        >
          {t("crashReviewDoNotSend")}
        </button>
      </div>
    </Dialog>
  );
}
