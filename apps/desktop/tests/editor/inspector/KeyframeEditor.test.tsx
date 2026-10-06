// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { t } from "../../../src/i18n";
import type { Keyframe, Position } from "../../../src/types";
import { F32KeyframeList, PositionKeyframeList } from "../../../src/editor/inspector/KeyframeEditor";

afterEach(() => {
  return cleanup();
});

const numbers = (): Keyframe<number>[] => {
  return [
    { time_fraction: 0, value: 0.2 },
    { time_fraction: 1, value: 0.8 },
  ];
};

function renderNumbers(onCommit = vi.fn(), keyframes = numbers()) {
  const view = render(
    <F32KeyframeList
      label="Opacity"
      keyframes={keyframes}
      min={0}
      max={1}
      step={0.01}
      defaultValue={0.5}
      onCommit={onCommit}
    />,
  );
  return { onCommit, ...view };
}

describe("F32KeyframeList", () => {
  it("shows the label and one row per keyframe", () => {
    renderNumbers();
    expect(screen.getByText("Opacity")).toBeTruthy();
    expect(screen.getAllByTitle(t("keyframeTime"))).toHaveLength(2);
  });

  it("previews a drag locally and commits the edited list once on release", () => {
    const { onCommit } = renderNumbers();
    const [time] = screen.getAllByTitle(t("keyframeTime"));
    fireEvent.change(time, { target: { value: "0.5" } });
    expect(onCommit).not.toHaveBeenCalled();
    fireEvent.pointerUp(time);
    expect(onCommit).toHaveBeenCalledTimes(1);
    expect(onCommit.mock.calls[0][0][0]).toEqual({ time_fraction: 0.5, value: 0.2 });
  });

  it("edits a keyframe value from the second slider and commits on blur", () => {
    const { onCommit, container } = renderNumbers();
    const sliders = container.querySelectorAll('input[type="range"]');
    // Per row: time, then value.
    fireEvent.change(sliders[1], { target: { value: "0.9" } });
    fireEvent.blur(sliders[1]);
    expect(onCommit.mock.calls[0][0][0]).toEqual({ time_fraction: 0, value: 0.9 });
  });

  it("adds a keyframe at the end with the default value and removes a row at once", () => {
    const { onCommit } = renderNumbers();
    fireEvent.click(screen.getByText(t("addKeyframe")));
    expect(onCommit).toHaveBeenLastCalledWith([...numbers(), { time_fraction: 1, value: 0.5 }]);

    fireEvent.click(screen.getAllByText("🗑")[0]);
    expect(onCommit).toHaveBeenLastCalledWith([numbers()[1], { time_fraction: 1, value: 0.5 }]);
  });

  it("follows new keyframes handed in by the parent", () => {
    const { rerender } = renderNumbers();
    rerender(
      <F32KeyframeList
        label="Opacity"
        keyframes={[{ time_fraction: 0.3, value: 0.1 }]}
        min={0}
        max={1}
        step={0.01}
        defaultValue={0.5}
        onCommit={vi.fn()}
      />,
    );
    expect(screen.getAllByTitle(t("keyframeTime"))).toHaveLength(1);
  });
});

const positions = (): Keyframe<Position>[] => {
  return [{ time_fraction: 0.5, value: { x: 0.1, y: -0.1 } }];
};

function renderPositions(onCommit = vi.fn()) {
  render(
    <PositionKeyframeList
      label="Position"
      keyframes={positions()}
      onCommit={onCommit}
    />,
  );
  return onCommit;
}

describe("PositionKeyframeList", () => {
  it("edits time, x and y independently and commits the whole list", () => {
    const onCommit = renderPositions();
    const time = screen.getByTitle(t("keyframeTime"));
    const x = screen.getByTitle(t("keyframeX"));
    const y = screen.getByTitle(t("keyframeY"));

    fireEvent.change(time, { target: { value: "0.75" } });
    fireEvent.pointerUp(time);
    expect(onCommit).toHaveBeenLastCalledWith([
      { time_fraction: 0.75, value: { x: 0.1, y: -0.1 } },
    ]);

    fireEvent.change(x, { target: { value: "0.4" } });
    fireEvent.blur(x);
    expect(onCommit.mock.calls.at(-1)![0][0].value).toEqual({ x: 0.4, y: -0.1 });

    fireEvent.change(y, { target: { value: "-0.6" } });
    fireEvent.pointerUp(y);
    expect(onCommit.mock.calls.at(-1)![0][0].value).toEqual({ x: 0.4, y: -0.6 });
  });

  it("adds a keyframe at the centre and removes rows", () => {
    const onCommit = renderPositions();
    fireEvent.click(screen.getByText(t("addKeyframe")));
    expect(onCommit).toHaveBeenLastCalledWith([
      ...positions(),
      { time_fraction: 1, value: { x: 0, y: 0 } },
    ]);
    fireEvent.click(screen.getAllByText("🗑")[0]);
    expect(onCommit).toHaveBeenLastCalledWith([{ time_fraction: 1, value: { x: 0, y: 0 } }]);
  });
});
