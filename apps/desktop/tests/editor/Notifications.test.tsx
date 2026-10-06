// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { t } from "../../src/i18n";
import {
  answerConfirm,
  answerExtra,
  askChoice,
  askConfirm,
  confirmStore,
  pushToast,
  toastStore,
} from "../../src/notifications";
import { uiStore } from "../../src/store";
import { ConfirmHost, ToastHost } from "../../src/editor/chrome/Notifications";

beforeEach(() => {
  vi.useFakeTimers();
  toastStore.set([]);
  confirmStore.set(null);
  uiStore.set({ ...uiStore.get(), error: null, openDialogs: 0 });
});
afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe("ToastHost", () => {
  it("shows a toast with the right role for its kind", () => {
    render(<ToastHost />);
    act(() => {
      pushToast("saved");
      pushToast("broke", "error");
    });
    expect(screen.getByRole("status").textContent).toContain("saved");
    expect(screen.getByRole("alert").textContent).toContain("broke");
  });

  it("turns uiStore.error into a toast and clears it", () => {
    render(<ToastHost />);
    act(() => {
      uiStore.set({ ...uiStore.get(), error: "disk full" });
    });
    expect(screen.getByRole("alert").textContent).toBe(t("errorPrefix") + ": disk full" + "");
    expect(uiStore.get().error).toBeNull();
  });

  it("dismisses on its close button and by itself after a while", () => {
    render(<ToastHost />);
    act(() => {
      pushToast("one");
      pushToast("two");
    });
    fireEvent.click(screen.getAllByLabelText(t("close"))[0]);
    expect(screen.queryByText("one")).toBeNull();
    act(() => {
      vi.advanceTimersByTime(8100);
    });
    expect(screen.queryByText("two")).toBeNull();
  });

  it("is a labelled live region even when empty", () => {
    render(<ToastHost />);
    const region = screen.getByRole("region", { name: t("notifications") });
    expect(region.getAttribute("aria-live")).toBe("polite");
  });
});

describe("ConfirmHost", () => {
  it("renders nothing without a question", () => {
    const { container } = render(<ConfirmHost />);
    expect(container.firstChild).toBeNull();
  });

  it("asks with named buttons and resolves with the choice", async () => {
    render(<ConfirmHost />);
    let answer: Promise<boolean> | undefined;
    act(() => {
      answer = askConfirm({
        title: "Sure?",
        message: "Really.",
        confirmLabel: "Do it",
        cancelLabel: "Never mind",
        danger: true,
      });
    });
    expect(screen.getByRole("dialog", { name: "Sure?" })).toBeTruthy();
    expect(screen.getByText("Do it").className).toContain("danger");
    fireEvent.click(screen.getByText("Do it"));
    expect(await answer).toBe(true);
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("resolves false on cancel and on Escape", async () => {
    render(<ConfirmHost />);
    let first: Promise<boolean> | undefined;
    act(() => {
      first = askConfirm({ title: "A", message: "m", confirmLabel: "Yes", cancelLabel: "No" });
    });
    fireEvent.click(screen.getByText("No"));
    expect(await first).toBe(false);
    let second: Promise<boolean> | undefined;
    act(() => {
      second = askConfirm({ title: "B", message: "m", confirmLabel: "Yes", cancelLabel: "No" });
    });
    fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
    expect(await second).toBe(false);
  });
});

describe("questions with a third answer", () => {
  it("shows the extra button and reports which answer was given", async () => {
    confirmStore.set(null);
    const asked = askChoice({
      title: "Quit?",
      message: "Unsaved.",
      confirmLabel: "Save",
      extraLabel: "Discard",
      cancelLabel: "Stay",
    });
    render(<ConfirmHost />);
    expect(screen.getByText("Discard")).toBeTruthy();
    fireEvent.click(screen.getByText("Discard"));
    await expect(asked).resolves.toBe("extra");
    expect(confirmStore.get()).toBeNull();
  });

  it("resolves confirm and cancel like a plain question", async () => {
    const first = askChoice({
      title: "t",
      message: "m",
      confirmLabel: "Save",
      extraLabel: "Discard",
      cancelLabel: "Stay",
    });
    answerConfirm(true);
    await expect(first).resolves.toBe("confirm");

    const second = askChoice({
      title: "t",
      message: "m",
      confirmLabel: "Save",
      extraLabel: "Discard",
      cancelLabel: "Stay",
    });
    answerConfirm(false);
    await expect(second).resolves.toBe("cancel");
  });

  it("answers an open question as cancelled when another one replaces it", async () => {
    const first = askChoice({
      title: "t",
      message: "m",
      confirmLabel: "a",
      extraLabel: "b",
      cancelLabel: "c",
    });
    void askConfirm({ title: "n", message: "n", confirmLabel: "y", cancelLabel: "n" });
    await expect(first).resolves.toBe("cancel");
  });

  it("does nothing when there is no extra answer to give", () => {
    confirmStore.set(null);
    expect(() => {
      answerExtra();
    }).not.toThrow();
  });
});
