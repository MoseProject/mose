// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { t } from "../i18n";
import { uiStore } from "../store";

vi.mock("../ipc", () => {
  return { api: { splitAtPlayhead: vi.fn().mockResolvedValue(undefined) } };
});

const { ToolRail } = await import("./ToolRail");
const { api } = await import("../ipc");

beforeEach(() => {
  vi.clearAllMocks();
  uiStore.set({ ...uiStore.get(), tool: "select", selectedClipIds: [] });
});
afterEach(() => {
  return cleanup();
});

describe("tool rail", () => {
  it("switches the active tool and marks it pressed", () => {
    render(<ToolRail />);
    fireEvent.click(screen.getByLabelText(t("toolRoll")));
    expect(uiStore.get().tool).toBe("roll");
    expect(screen.getByLabelText(t("toolRoll")).getAttribute("aria-pressed")).toBe("true");
    expect(screen.getByLabelText(t("toolSelect")).getAttribute("aria-pressed")).toBe("false");
    for (const key of ["toolSlip", "toolSlide", "toolText"] as const) {
      fireEvent.click(screen.getByLabelText(t(key)));
    }
    expect(uiStore.get().tool).toBe("text");
  });

  it("splits the selected clips at the playhead", () => {
    render(<ToolRail />);
    act(() => {
      return uiStore.set({ ...uiStore.get(), selectedClipIds: [1, 2] });
    });
    fireEvent.click(screen.getByLabelText(t("split")));
    expect(api.splitAtPlayhead).toHaveBeenCalledWith([1, 2]);
  });
});

describe("trim tools group", () => {
  it("shares one slot for roll, slip and slide", () => {
    const { container } = render(<ToolRail />);
    expect(container.querySelectorAll(".trim-group")).toHaveLength(1);
    expect(screen.getByLabelText(t("trimTools"))).toBeTruthy();
  });

  it("shows the active trim tool and selects one from the menu", () => {
    const { container } = render(<ToolRail />);
    expect(container.querySelector(".trim-group.active")).toBeNull();
    fireEvent.click(screen.getAllByLabelText(t("toolSlide"))[0]);
    expect(uiStore.get().tool).toBe("slide");
    expect(container.querySelector(".trim-group.active")).toBeTruthy();
    expect(screen.getAllByLabelText(t("toolSlide"))[0].getAttribute("aria-pressed")).toBe("true");
  });

  it("remembers the last trim tool after returning to Select", () => {
    render(<ToolRail />);
    fireEvent.click(screen.getAllByLabelText(t("toolSlip"))[0]);
    fireEvent.click(screen.getByLabelText(t("toolSelect")));
    expect(uiStore.get().tool).toBe("select");
    expect(screen.getByLabelText(t("trimTools"))).toBeTruthy();
  });

  it("puts the key binding in the tooltips", () => {
    render(<ToolRail />);
    expect(screen.getByLabelText(t("toolSelect")).getAttribute("title")).toContain("(V)");
    expect(screen.getByLabelText(t("toolText")).getAttribute("title")).toContain("(T)");
    expect(screen.getByLabelText(t("split")).getAttribute("title")).toContain("(S)");
  });
});
