// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { t } from "../../src/i18n";

vi.mock("../../src/ipc", () => {
  return {
    api: {
      getPendingCrashReview: vi.fn(),
      sendCrashOnce: vi.fn().mockResolvedValue(undefined),
      alwaysSendCrash: vi.fn().mockResolvedValue(undefined),
      dismissCrashReview: vi.fn().mockResolvedValue(undefined),
    },
  };
});

const { CrashReviewModal } = await import("../../src/screens/CrashReviewModal");
const { api } = await import("../../src/ipc");

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(api.getPendingCrashReview).mockResolvedValue({
    payloadPreview: "stack trace",
  } as never);
});
afterEach(() => {
  return cleanup();
});

describe("crash review modal", () => {
  it("renders nothing when no crash is pending", async () => {
    vi.mocked(api.getPendingCrashReview).mockResolvedValue(undefined as never);
    const { container } = render(<CrashReviewModal />);
    await waitFor(() => {
      return expect(api.getPendingCrashReview).toHaveBeenCalled();
    });
    expect(container.innerHTML).toBe("");
  });

  it("toggles the payload preview", async () => {
    render(<CrashReviewModal />);
    await screen.findByText(t("crashReviewOffer"));
    expect(screen.queryByText("stack trace")).toBeNull();
    fireEvent.click(screen.getByText(t("crashReviewShowPayload")));
    expect(screen.getByText("stack trace")).toBeTruthy();
    fireEvent.click(screen.getByText(t("crashReviewShowPayload")));
    expect(screen.queryByText("stack trace")).toBeNull();
  });

  it.each([
    ["crashReviewSendOnce", "sendCrashOnce"],
    ["crashReviewAlwaysSend", "alwaysSendCrash"],
    ["crashReviewDoNotSend", "dismissCrashReview"],
  ] as const)("%s runs its action and closes the modal", async (label, method) => {
    const { container } = render(<CrashReviewModal />);
    fireEvent.click(await screen.findByText(t(label)));
    expect(api[method]).toHaveBeenCalledTimes(1);
    expect(container.innerHTML).toBe("");
  });
});
