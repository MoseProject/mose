// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { t } from "../i18n";
import { uiStore } from "../store";

vi.mock("../ipc", () => ({ api: { splitAtPlayhead: vi.fn().mockResolvedValue(undefined) } }));

const { ToolRail } = await import("./ToolRail");
const { api } = await import("../ipc");

beforeEach(() => {
  vi.clearAllMocks();
  uiStore.set({ ...uiStore.get(), tool: "select", selectedClipIds: [] });
});
afterEach(() => cleanup());

describe("tool rail", () => {
  it("switches the active tool and marks it pressed", () => {
    render(<ToolRail />);
    fireEvent.click(screen.getByLabelText(t("toolRoll")));
    expect(uiStore.get().tool).toBe("roll");
    expect(screen.getByLabelText(t("toolRoll")).getAttribute("aria-pressed")).toBe("true");
    expect(screen.getByLabelText(t("toolSelect")).getAttribute("aria-pressed")).toBe("false");
    for (const key of ["toolSlip", "toolSlide", "toolText"] as const) fireEvent.click(screen.getByLabelText(t(key)));
    expect(uiStore.get().tool).toBe("text");
  });

  it("splits the selected clips at the playhead", () => {
    render(<ToolRail />);
    act(() => uiStore.set({ ...uiStore.get(), selectedClipIds: [1, 2] }));
    fireEvent.click(screen.getByLabelText(t("split")));
    expect(api.splitAtPlayhead).toHaveBeenCalledWith([1, 2]);
  });
});
