import { beforeEach, describe, expect, it } from "vitest";
import {
  answerConfirm,
  askConfirm,
  confirmStore,
  dismissToast,
  pushToast,
  toastStore,
} from "../src/notifications";

const question = {
  title: "Replace?",
  message: "It exists.",
  confirmLabel: "Replace",
  cancelLabel: "Keep both",
};

beforeEach(() => {
  toastStore.set([]);
  confirmStore.set(null);
});

describe("toasts", () => {
  it("stacks in order with distinct ids and a kind", () => {
    const first = pushToast("one", "error");
    const second = pushToast("two");
    expect(second).toBeGreaterThan(first);
    expect(toastStore.get()).toEqual([
      { id: first, kind: "error", message: "one" },
      { id: second, kind: "info", message: "two" },
    ]);
  });

  it("keeps the newest four", () => {
    for (let i = 0; i < 6; i += 1) {
      pushToast("m" + i);
    }
    expect(
      toastStore.get().map((toast) => {
        return toast.message;
      }),
    ).toEqual(["m2", "m3", "m4", "m5"]);
  });

  it("dismisses one toast and ignores an unknown id", () => {
    const keep = pushToast("keep");
    const drop = pushToast("drop");
    dismissToast(drop);
    dismissToast(9999);
    expect(
      toastStore.get().map((toast) => {
        return toast.id;
      }),
    ).toEqual([keep]);
  });
});

describe("askConfirm", () => {
  it("resolves true when answered yes and false when answered no", async () => {
    const yes = askConfirm(question);
    expect(confirmStore.get()?.title).toBe("Replace?");
    answerConfirm(true);
    expect(await yes).toBe(true);
    expect(confirmStore.get()).toBeNull();

    const no = askConfirm(question);
    answerConfirm(false);
    expect(await no).toBe(false);
  });

  it("cancels the open question when a second one is asked", async () => {
    const first = askConfirm(question);
    const second = askConfirm({ ...question, title: "Other" });
    expect(await first).toBe(false);
    expect(confirmStore.get()?.title).toBe("Other");
    answerConfirm(true);
    expect(await second).toBe(true);
  });

  it("does nothing when answered with no question open", () => {
    expect(() => {
      answerConfirm(true);
    }).not.toThrow();
  });
});
