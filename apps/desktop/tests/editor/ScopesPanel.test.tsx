// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { SCOPE_REFRESH_MS, ScopesPanel } from "../../src/editor/ScopesPanel";
import { t } from "../../src/i18n";
import { uiStore } from "../../src/store";

beforeEach(() => {
  vi.useFakeTimers();
  uiStore.set({ ...uiStore.get(), scopesOpen: true, scopeKind: "waveform" });
});
afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

const source = () => {
  return screen.getByTestId("scope-image").getAttribute("src") ?? "";
};

describe("ScopesPanel", () => {
  it("asks for the scope that is selected, of the current frame", () => {
    render(<ScopesPanel frameSeq={4} />);
    expect(source()).toMatch(/\/scope\/waveform\/4$/);
    fireEvent.change(screen.getByLabelText(t("scopes")), { target: { value: "parade" } });
    expect(source()).toMatch(/\/scope\/parade\/4$/);
    expect(uiStore.get().scopeKind).toBe("parade");
  });

  it("offers the four scopes, named", () => {
    render(<ScopesPanel frameSeq={1} />);
    const names = Array.from((screen.getByLabelText(t("scopes")) as HTMLSelectElement).options).map(
      (option) => {
        return option.textContent;
      },
    );
    expect(names).toEqual([
      t("scopeWaveform"),
      t("scopeParade"),
      t("scopeHistogram"),
      t("scopeVectorscope"),
    ]);
  });

  it("follows new frames, but no faster than the refresh interval", () => {
    const { rerender } = render(<ScopesPanel frameSeq={1} />);
    // The first new frame is asked for right away (nothing was asked recently).
    rerender(<ScopesPanel frameSeq={2} />);
    act(() => {
      vi.advanceTimersByTime(1);
    });
    expect(source()).toMatch(/\/2$/);
    // A burst of frames during playback is held back and only the newest is asked for.
    rerender(<ScopesPanel frameSeq={3} />);
    rerender(<ScopesPanel frameSeq={4} />);
    rerender(<ScopesPanel frameSeq={5} />);
    expect(source()).toMatch(/\/2$/);
    act(() => {
      vi.advanceTimersByTime(SCOPE_REFRESH_MS + 5);
    });
    expect(source()).toMatch(/\/5$/);
  });
});
