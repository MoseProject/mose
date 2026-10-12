// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  formatCorner,
  formatEq,
  formatPan,
  formatRatio,
  formatVolume,
  TrackMixButton,
} from "../../../src/editor/timeline/TrackMixButton";
import { t } from "../../../src/i18n";
import { NEUTRAL_TRACK_MIX, type Track, type TrackMix } from "../../../src/types";

const calls = vi.hoisted(() => {
  return { setTrackMix: vi.fn().mockResolvedValue(undefined) };
});
vi.mock("../../../src/ipc", () => {
  return { api: calls };
});

const track = (mix: Partial<TrackMix>) => {
  return {
    id: 7,
    name: "A1",
    kind: "Audio",
    mix: { ...NEUTRAL_TRACK_MIX, ...mix },
  } as unknown as Track;
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

  it("prints the tone and dynamics controls", () => {
    expect(formatEq(4.5)).toBe("+4.5 dB");
    expect(formatEq(-3)).toBe("-3.0 dB");
    expect(formatCorner(0)).toBe(t("trackFilterOff"));
    expect(formatCorner(80)).toBe("80 Hz");
    expect(formatCorner(8000)).toBe("8.0 kHz");
    expect(formatRatio(1)).toBe(t("trackCompOff"));
    expect(formatRatio(4)).toBe("4.0:1");
  });
});

describe("TrackMixButton", () => {
  it("is closed until clicked and lit only while the mix is not neutral", () => {
    const { rerender } = render(<TrackMixButton track={track({})} />);
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
    expect(calls.setTrackMix).toHaveBeenLastCalledWith(7, {
      ...track({ pan: 0.5 }).mix,
      volume_db: -9,
    });
    drag(t("trackPan"), -0.25);
    expect(calls.setTrackMix).toHaveBeenLastCalledWith(7, {
      ...track({ volume_db: -3 }).mix,
      pan: -0.25,
    });
  });

  it("lights up for any tone or dynamics control, and a lone threshold does not count", () => {
    const { rerender } = render(<TrackMixButton track={track({ eq_low_db: 3 })} />);
    expect(screen.getByTitle(t("trackMix")).className).toContain("active");
    rerender(<TrackMixButton track={track({ highpass_hz: 80 })} />);
    expect(screen.getByTitle(t("trackMix")).className).toContain("active");
    rerender(<TrackMixButton track={track({ comp_ratio: 3 })} />);
    expect(screen.getByTitle(t("trackMix")).className).toContain("active");
    rerender(<TrackMixButton track={track({ comp_threshold_db: -40 })} />);
    expect(screen.getByTitle(t("trackMix")).className).not.toContain("active");
  });

  it("commits each tone and dynamics control keeping the rest of the strip", () => {
    render(<TrackMixButton track={track({ volume_db: -3, eq_mid_db: 2 })} />);
    open();
    const base = track({ volume_db: -3, eq_mid_db: 2 }).mix;
    drag(t("trackEqLow"), 6);
    expect(calls.setTrackMix).toHaveBeenLastCalledWith(7, { ...base, eq_low_db: 6 });
    drag(t("trackHighpass"), 120);
    expect(calls.setTrackMix).toHaveBeenLastCalledWith(7, { ...base, highpass_hz: 120 });
    drag(t("trackCompRatio"), 4);
    expect(calls.setTrackMix).toHaveBeenLastCalledWith(7, { ...base, comp_ratio: 4 });
    drag(t("trackLowpass"), 3000);
    expect(calls.setTrackMix).toHaveBeenLastCalledWith(7, { ...base, lowpass_hz: 3000 });
  });

  it("treats the low-pass slider's far end as off, which the backend knows as 0", () => {
    render(<TrackMixButton track={track({ lowpass_hz: 3000 })} />);
    open();
    drag(t("trackLowpass"), 20000);
    expect(calls.setTrackMix).toHaveBeenLastCalledWith(7, { ...track({}).mix, lowpass_hz: 0 });
  });

  it("resets to neutral, and the reset is disabled when it already is", () => {
    const { rerender } = render(<TrackMixButton track={track({ volume_db: -12, pan: 1 })} />);
    open();
    fireEvent.click(screen.getByTestId("track-mix-reset"));
    expect(calls.setTrackMix).toHaveBeenLastCalledWith(7, NEUTRAL_TRACK_MIX);
    rerender(<TrackMixButton track={track({})} />);
    expect((screen.getByTestId("track-mix-reset") as HTMLButtonElement).disabled).toBe(true);
  });

  it("closes on Escape and on a click outside", () => {
    render(<TrackMixButton track={track({})} />);
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
