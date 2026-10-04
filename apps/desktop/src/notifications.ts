// Transient messages and questions: toasts for things that happened in the background, and an
// in-app confirmation that replaces the system window.confirm (which cannot be themed, named or
// translated per button).

import { createStore } from "./store";

export type ToastKind = "error" | "info";

export interface Toast {
  id: number;
  kind: ToastKind;
  message: string;
}

export const toastStore = createStore<Toast[]>([]);
const MAX_TOASTS = 4;
let nextToastId = 1;

/** Shows a toast; the oldest is dropped past four. Returns its id. */
export function pushToast(message: string, kind: ToastKind = "info"): number {
  const id = nextToastId;
  nextToastId += 1;
  toastStore.set((toasts) => {
    return [...toasts, { id, kind, message }].slice(-MAX_TOASTS);
  });
  return id;
}

export function dismissToast(id: number) {
  toastStore.set((toasts) => {
    return toasts.filter((toast) => {
      return toast.id !== id;
    });
  });
}

export interface ConfirmRequest {
  title: string;
  message: string;
  confirmLabel: string;
  cancelLabel: string;
  /** Styles the confirm button as destructive. */
  danger?: boolean;
  resolve: (confirmed: boolean) => void;
}

export const confirmStore = createStore<ConfirmRequest | null>(null);

/** Asks the user a yes/no question in a dialog. Resolves `false` when it is dismissed. A second
 *  question while one is open cancels the first. */
export function askConfirm(options: Omit<ConfirmRequest, "resolve">): Promise<boolean> {
  return new Promise((resolve) => {
    confirmStore.get()?.resolve(false);
    confirmStore.set({ ...options, resolve });
  });
}

/** Answers the open question and closes it. */
export function answerConfirm(confirmed: boolean) {
  const request = confirmStore.get();
  confirmStore.set(null);
  request?.resolve(confirmed);
}
