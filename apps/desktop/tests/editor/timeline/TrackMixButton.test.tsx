// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  formatPan,
  formatVolume,
  TrackMixButton,
} from "../../../src/editor/timeline/TrackMixButton";
import { t } from "../../../src/i18n";
import type { Track } from "../../../src/types";

const calls = vi.hoisted(() => {
  return { setTrackMix: vi.fn().mockResolvedValue(undefined) };
});
vi.mock("../../../src/ipc", () => {
  return { api: calls };
});

const track = (mix: { volume_db: number; pan: number }) => {
  return { id: 7, name: "A1", kind: "Audio", mix } as unknown as Track;
};

beforeEach(() => {
  calls.setTrackMix.mockClear();
});
afterEach(cleanup);

const open = () => {
  fireEvent.click(screen.getByTitle(t("trackMix")));
};

/** Drags a range input to `value` and lets go, the way the sliders commit. */
const drag = (label: string, value: number) => {
  const input = screen.getByLabelText(label);
  fireEvent.change(input, { target: { value: String(value) } });
  fireEvent.pointerUp(input);
};

describe("track mix formatting", () => {
  it("prints the fader in dB and the pan as a mixer does", () => {
    expect(formatVolume(0)).toBe("0.0 dB");
    expect(formatVolume(3)).toBe("+3.0 dB");
    expect(formatVolume(-6.5)).toBe("-6.5 dB");
    expect(formatPan(0)).toBe("C");
    expect(formatPan(-0.4)).toBe("L40");
    expect(formatPan(1)).toBe("R100");
    expect(formatPan(0.004)).toBe("C");
  });
});

describe("TrackMixButton", () => {
  it("is closed until clicked and lit only while the mix is not neutral", () => {
    const { rerender } = render(<TrackMixButton track={track({ volume_db: 0, pan: 0 })} />);
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(screen.getByTitle(t("trackMix")).className).not.toContain("active");
    rerender(<TrackMixButton track={track({ volume_db: -6, pan: 0 })} />);
    expect(screen.getByTitle(t("trackMix")).className).toContain("active");
    rerender(<TrackMixButton track={track({ volume_db: 0, pan: 0.3 })} />);
    expect(screen.getByTitle(t("trackMix")).className).toContain("active");
  });

  it("commits the fader and the pan together, each keeping the other's value", () => {
    render(<TrackMixButton track={track({ volume_db: -3, pan: 0.5 })} />);
    open();
    expect(screen.getByRole("dialog")).toBeTruthy();
    drag(t("trackVolume"), -9);
    expect(calls.setTrackMix).toHaveBeenLastCalledWith(7, -9, 0.5);
    drag(t("trackPan"), -0.25);
    expect(calls.setTrackMix).toHaveBeenLastCalledWith(7, -3, -0.25);
  });

  it("resets to neutral, and the reset is disabled when it already is", () => {
    const { rerender } = render(<TrackMixButton track={track({ volume_db: -12, pan: 1 })} />);
    open();
    fireEvent.click(screen.getByTestId("track-mix-reset"));
    expect(calls.setTrackMix).toHaveBeenLastCalledWith(7, 0, 0);
    rerender(<TrackMixButton track={track({ volume_db: 0, pan: 0 })} />);
    expect((screen.getByTestId("track-mix-reset") as HTMLButtonElement).disabled).toBe(true);
  });

  it("closes on Escape and on a click outside", () => {
    render(<TrackMixButton track={track({ volume_db: 0, pan: 0 })} />);
    open();
    fireEvent.keyDown(document, { key: "Escape" });
    expect(screen.queryByRole("dialog")).toBeNull();
    open();
    fireEvent.pointerDown(document.body);
    expect(screen.queryByRole("dialog")).toBeNull();
    open();
    fireEvent.pointerDown(screen.getByRole("dialog"));
    expect(screen.getByRole("dialog")).toBeTruthy();
  });

  it("copes with a track that has no mix yet", () => {
    render(<TrackMixButton track={{ id: 1, name: "A", kind: "Audio" } as unknown as Track} />);
    open();
    expect((screen.getByTestId("track-mix-reset") as HTMLButtonElement).disabled).toBe(true);
  });
});
