// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { t } from "../../src/i18n";
import { makeSession } from "../support/testSupport";
import type { ClipInstance, SessionView } from "../../src/types";

vi.mock("../../src/ipc", () => {
  return { api: { updateClip: vi.fn().mockResolvedValue(undefined) } };
});

const { EffectsBrowser, filterCatalog } = await import("../../src/editor/library/EffectsBrowser");
const { api } = await import("../../src/ipc");
const { patchUi } = await import("../../src/store");

const session = (): SessionView => {
  return makeSession([
    {
      id: 1,
      name: "Main",
      tracks: [
        {
          clips: [{ id: 7 } as ClipInstance, { id: 8 } as ClipInstance, { id: 9 } as ClipInstance],
          text_clips: [{ id: 20 }],
          shape_clips: [],
        },
      ],
    },
  ]);
};

beforeEach(() => {
  vi.clearAllMocks();
  patchUi({ selectedClipIds: [7] });
});
afterEach(() => {
  cleanup();
});

describe("EffectsBrowser", () => {
  it("applies the picked effect to the selected video clip", () => {
    render(<EffectsBrowser session={session()} />);
    fireEvent.click(screen.getByTestId("effect-transitions-Slide"));
    expect(api.updateClip).toHaveBeenCalledWith(7, { transitionIn: "Slide" });
    fireEvent.click(screen.getByTestId("effect-masks-Circle"));
    expect(api.updateClip).toHaveBeenCalledWith(7, { maskShape: "Circle" });
  });

  it("applies it to every selected video clip and leaves overlays alone", async () => {
    patchUi({ selectedClipIds: [7, 9, 20] });
    render(<EffectsBrowser session={session()} />);
    fireEvent.click(screen.getByTestId("effect-transitions-Fade"));
    await vi.waitFor(() => {
      expect(api.updateClip).toHaveBeenCalledTimes(2);
    });
    expect(api.updateClip).toHaveBeenCalledWith(7, { transitionIn: "Fade" });
    expect(api.updateClip).toHaveBeenCalledWith(9, { transitionIn: "Fade" });
  });

  it("asks for a clip and disables the effects while none is selected", () => {
    patchUi({ selectedClipIds: [] });
    render(<EffectsBrowser session={session()} />);
    expect(screen.getByText(t("effectsNeedClip"))).toBeTruthy();
    const item = screen.getByTestId("effect-transitions-Fade") as HTMLButtonElement;
    expect(item.disabled).toBe(true);
    fireEvent.click(item);
    expect(api.updateClip).not.toHaveBeenCalled();
  });

  it("filters by effect or folder name and says when nothing matches", () => {
    render(<EffectsBrowser session={session()} />);
    const search = screen.getByLabelText(t("searchEffects"));
    fireEvent.change(search, { target: { value: "multiply" } });
    expect(screen.getByTestId("effect-blend-Multiply")).toBeTruthy();
    expect(screen.queryByTestId("effect-transitions-Fade")).toBeNull();
    fireEvent.change(search, { target: { value: t("effectsMasks").toLowerCase() } });
    expect(screen.getByTestId("effect-masks-Circle")).toBeTruthy();
    expect(screen.getByTestId("effect-masks-RoundedRect")).toBeTruthy();
    fireEvent.change(search, { target: { value: "zzzz" } });
    expect(screen.getByText(t("effectsNoMatches"))).toBeTruthy();
  });
});

describe("filterCatalog", () => {
  const folders = [
    { id: "a", label: "Alpha", effects: [{ id: "1", label: "One", patch: {} }] },
    { id: "b", label: "Beta", effects: [{ id: "2", label: "Two", patch: {} }] },
  ];
  it("returns everything for a blank query and drops empty folders otherwise", () => {
    expect(filterCatalog(folders, "  ")).toBe(folders);
    expect(
      filterCatalog(folders, "two").map((f) => {
        return f.id;
      }),
    ).toEqual(["b"]);
  });
});
