import { useEffect } from "react";
import { t } from "../i18n";
import {
  answerConfirm,
  answerExtra,
  confirmStore,
  dismissToast,
  pushToast,
  toastStore,
} from "../notifications";
import { patchUi, uiStore, useStore } from "../store";
import { Dialog } from "./Dialog";
import { Icon } from "./Icon";

const TOAST_MS = 8000;

/** Renders the confirmation dialog when a question is pending. */
export function ConfirmHost() {
  const request = useStore(confirmStore);
  if (!request) {
    return null;
  }
  return (
    <Dialog
      title={request.title}
      onClose={() => {
        answerConfirm(false);
      }}
      actions={
        <>
          <button
            className="icon"
            onClick={() => {
              answerConfirm(false);
            }}
          >
            {request.cancelLabel}
          </button>
          {request.extraLabel && (
            <button
              className="danger"
              onClick={() => {
                answerExtra();
              }}
            >
              {request.extraLabel}
            </button>
          )}
          <button
            className={request.danger && !request.extraLabel ? "danger" : "primary"}
            onClick={() => {
              answerConfirm(true);
            }}
          >
            {request.confirmLabel}
          </button>
        </>
      }
    >
      <p>{request.message}</p>
    </Dialog>
  );
}

/** Stacks toasts bottom-right. Errors reported through uiStore.error become toasts here, so one
 *  failure never replaces another. */
export function ToastHost() {
  const toasts = useStore(toastStore);
  const error = useStore(uiStore).error;

  useEffect(() => {
    if (error) {
      pushToast(t("errorPrefix") + ": " + error, "error");
      patchUi({ error: null });
    }
  }, [error]);

  useEffect(() => {
    const timers = toasts.map((toast) => {
      return window.setTimeout(() => {
        dismissToast(toast.id);
      }, TOAST_MS);
    });
    return () => {
      timers.forEach((timer) => {
        window.clearTimeout(timer);
      });
    };
  }, [toasts]);

  return (
    <div
      className="toast-stack"
      role="region"
      aria-label={t("notifications")}
      aria-live="polite"
    >
      {toasts.map((toast) => {
        return (
          <div
            key={toast.id}
            className={"toast " + toast.kind}
            role={toast.kind === "error" ? "alert" : "status"}
          >
            <span>{toast.message}</span>
            <button
              className="icon"
              aria-label={t("close")}
              onClick={() => {
                dismissToast(toast.id);
              }}
            >
              <Icon name="close" />
            </button>
          </div>
        );
      })}
    </div>
  );
}
