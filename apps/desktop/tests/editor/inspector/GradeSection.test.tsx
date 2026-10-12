// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { GradeSection, gradeIsNeutral } from "../../../src/editor/inspector/GradeSection";
import { t } from "../../../src/i18n";
import { NEUTRAL_GRADE, type ClipInstance } from "../../../src/types";

const commit = vi.fn().mockResolvedValue(undefined);

const clip = (grade: Partial<typeof NEUTRAL_GRADE> = {}) => {
  return { grade: { ...NEUTRAL_GRADE, ...grade } } as unknown as ClipInstance;
};

/** Opens the section: its body is built the first time it is open. */
const open = () => {
  const details = document.querySelector("details") as HTMLDetailsElement;
  details.open = true;
  fireEvent(details, new Event("toggle"));
};

/** Drags a range input to `value` and lets go, the way the sliders commit. */
const drag = (label: string, value: number) => {
  const input = screen.getByLabelText(label);
  fireEvent.change(input, { target: { value: String(value) } });
  fireEvent.pointerUp(input);
};

beforeEach(() => {
  commit.mockClear();
});
afterEach(cleanup);

describe("GradeSection", () => {
  it("tells a neutral grade from a graded one", () => {
    expect(gradeIsNeutral(NEUTRAL_GRADE)).toBe(true);
    expect(gradeIsNeutral({ ...NEUTRAL_GRADE, exposure: 0.5 })).toBe(false);
    expect(gradeIsNeutral({ ...NEUTRAL_GRADE, gain: [1, 1, 1.1] })).toBe(false);
  });

  it("commits the whole grade with the one control that moved", () => {
    render(
      <GradeSection
        clip={clip({ tint: 0.2 })}
        commit={commit}
      />,
    );
    drag(t("gradeExposure"), 1.5);
    expect(commit).toHaveBeenLastCalledWith({
      grade: { ...NEUTRAL_GRADE, tint: 0.2, exposure: 1.5 },
    });
    drag(t("gradeTemperature"), -0.4);
    expect(commit).toHaveBeenLastCalledWith({
      grade: { ...NEUTRAL_GRADE, tint: 0.2, temperature: -0.4 },
    });
  });

  it("moves one channel of lift, gamma or gain without touching the other two", () => {
    render(
      <GradeSection
        clip={clip({ gain: [1, 1, 1] })}
        commit={commit}
      />,
    );
    open();
    drag(`${t("gradeGain")} ${t("gradeBlue")}`, 0.8);
    expect(commit).toHaveBeenLastCalledWith({
      grade: { ...NEUTRAL_GRADE, gain: [1, 1, 0.8] },
    });
    drag(`${t("gradeLift")} ${t("gradeRed")}`, 0.1);
    expect(commit).toHaveBeenLastCalledWith({
      grade: { ...NEUTRAL_GRADE, lift: [0.1, 0, 0] },
    });
    drag(`${t("gradeGamma")} ${t("gradeGreen")}`, 1.4);
    expect(commit).toHaveBeenLastCalledWith({
      grade: { ...NEUTRAL_GRADE, gamma: [1, 1.4, 1] },
    });
  });

  it("copes with a clip that has no grade yet", () => {
    render(
      <GradeSection
        clip={{} as unknown as ClipInstance}
        commit={commit}
      />,
    );
    open();
    drag(t("gradeShadows"), 0.5);
    expect(commit).toHaveBeenLastCalledWith({ grade: { ...NEUTRAL_GRADE, shadows: 0.5 } });
  });
});
