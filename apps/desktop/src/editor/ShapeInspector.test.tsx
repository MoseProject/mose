// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { t } from "../i18n";
import type { OverlayCatalog, ShapeClip, ShapeKind } from "../types";

vi.mock("../ipc", () => {
  return {
    api: {
      overlayCatalog: vi.fn(),
      updateShapeClip: vi.fn().mockResolvedValue(undefined),
    },
  };
});

const { ShapeInspector } = await import("./ShapeInspector");
const { api } = await import("../ipc");

const rectangle: ShapeKind = {
  Polygon: [
    [0, 0],
    [1, 0],
    [1, 1],
    [0, 1],
  ],
};
const triangle: ShapeKind = {
  Polygon: [
    [0.5, 0],
    [1, 1],
    [0, 1],
  ],
};

const catalog = {
  fonts: [],
  shapePresets: [
    { preset: "Ellipse", kind: "Ellipse" },
    { preset: "Rectangle", kind: rectangle },
    { preset: "Triangle", kind: triangle },
  ],
} as unknown as OverlayCatalog;

const clip = (overrides: Partial<ShapeClip> = {}): ShapeClip => {
  return {
    id: 8,
    start_secs: 1,
    duration_secs: 3,
    shape_kind: rectangle,
    center_x: 0.5,
    center_y: 0.5,
    center_x_keyframes: [],
    center_y_keyframes: [],
    width: 0.4,
    height: 0.3,
    width_keyframes: [],
    height_keyframes: [],
    rotation_deg: 10,
    rotation_keyframes: [],
    stroke_thickness_px: 2,
    color_rgba: [255, 0, 0, 128],
    ...overrides,
  };
};

const field = (label: string) => {
  return screen.getByText(label, { selector: "label" }).parentElement as HTMLElement;
};
const control = (label: string, selector: string) => {
  return field(label).querySelector(selector) as HTMLInputElement | HTMLSelectElement;
};
const lastPatch = () => {
  return vi.mocked(api.updateShapeClip).mock.calls.at(-1)![1];
};

function slide(label: string, value: number) {
  const range = control(label, 'input[type="range"]');
  fireEvent.change(range, { target: { value: String(value) } });
  fireEvent.pointerUp(range);
}

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(api.overlayCatalog).mockResolvedValue(catalog);
});
afterEach(() => {
  return cleanup();
});

async function loaded(overrides: Partial<ShapeClip> = {}) {
  const view = render(<ShapeInspector clip={clip(overrides)} />);
  await waitFor(() => {
    return expect((control(t("shapeKind"), "select") as HTMLSelectElement).disabled).toBe(false);
  });
  return view;
}

describe("shape inspector", () => {
  it("keeps the shape picker disabled until the catalog loads", async () => {
    render(<ShapeInspector clip={clip()} />);
    expect((control(t("shapeKind"), "select") as HTMLSelectElement).disabled).toBe(true);
    await waitFor(() => {
      return expect((control(t("shapeKind"), "select") as HTMLSelectElement).disabled).toBe(false);
    });
  });

  it("selects the matching preset and switches to another", async () => {
    await loaded();
    const select = control(t("shapeKind"), "select") as HTMLSelectElement;
    expect(select.value).toBe("Rectangle");
    expect(
      [...select.options].map((o) => {
        return o.value;
      }),
    ).toEqual(["Ellipse", "Rectangle", "Triangle"]);
    fireEvent.change(select, { target: { value: "Triangle" } });
    expect(lastPatch()).toEqual({ shapeKind: "Triangle" });
  });

  it("shows a non-pickable Custom entry for hand-edited geometry", async () => {
    await loaded({
      shape_kind: {
        Polygon: [
          [0, 0],
          [1, 0],
          [0.3, 0.9],
        ],
      },
    });
    const custom = screen.getByText(t("shapeCustom")) as HTMLOptionElement;
    expect(custom.disabled).toBe(true);
    expect((control(t("shapeKind"), "select") as HTMLSelectElement).value).toBe("Custom");
  });

  it("has no vertex editor for an ellipse", async () => {
    await loaded({ shape_kind: "Ellipse" });
    expect(screen.queryByText(t("shapeVertices"))).toBeNull();
  });

  it("edits a single vertex coordinate and keeps the others", async () => {
    await loaded();
    const inputs = screen
      .getByText(t("shapeVertices"))
      .closest("details")!
      .querySelectorAll('input[type="number"]') as NodeListOf<HTMLInputElement>;
    // Vertex 2 is (1, 0): inputs 2 and 3.
    fireEvent.change(inputs[2], { target: { value: "0.8" } });
    expect(lastPatch()).toEqual({
      polygonVertices: [
        [0, 0],
        [0.8, 0],
        [1, 1],
        [0, 1],
      ],
    });
    fireEvent.change(inputs[3], { target: { value: "0.2" } });
    expect(lastPatch()).toEqual({
      polygonVertices: [
        [0, 0],
        [1, 0.2],
        [1, 1],
        [0, 1],
      ],
    });
  });

  it("removes a vertex but never below three", async () => {
    await loaded();
    const section = screen.getByText(t("shapeVertices")).closest("details")!;
    fireEvent.click(section.querySelectorAll("button")[0]);
    expect(lastPatch()).toEqual({
      polygonVertices: [
        [1, 0],
        [1, 1],
        [0, 1],
      ],
    });
    cleanup();

    await loaded({ shape_kind: triangle });
    const buttons = screen
      .getByText(t("shapeVertices"))
      .closest("details")!
      .querySelectorAll("button") as NodeListOf<HTMLButtonElement>;
    expect(
      [...buttons].every((b) => {
        return b.disabled;
      }),
    ).toBe(true);
  });

  it("commits color, opacity, position and size", async () => {
    await loaded();
    fireEvent.change(control(t("shapeColor"), 'input[type="color"]'), {
      target: { value: "#00ff00" },
    });
    expect(lastPatch()).toEqual({ colorRgb: [0, 255, 0] });
    slide(t("opacity"), 0.5);
    expect(lastPatch()).toEqual({ opacity: 128 });
    slide(t("positionX"), 0.2);
    expect(lastPatch()).toEqual({ centerX: 0.2 });
    slide(t("positionY"), 0.8);
    expect(lastPatch()).toEqual({ centerY: 0.8 });
    slide(t("width"), 0.6);
    expect(lastPatch()).toEqual({ width: 0.6 });
    slide(t("height"), 0.7);
    expect(lastPatch()).toEqual({ height: 0.7 });
    slide(t("shapeRotation"), 90);
    expect(lastPatch()).toEqual({ rotationDeg: 90 });
  });

  it("commits stroke thickness and timing on blur", async () => {
    await loaded();
    const stroke = control(t("shapeStroke"), "input");
    fireEvent.change(stroke, { target: { value: "3.5" } });
    fireEvent.blur(stroke);
    expect(lastPatch()).toEqual({ strokeThicknessPx: 3.5 });

    const start = control(t("overlayStart"), "input");
    fireEvent.change(start, { target: { value: "2" } });
    fireEvent.blur(start);
    expect(lastPatch()).toEqual({ startSecs: 2 });

    const duration = control(t("overlayDuration"), "input");
    fireEvent.change(duration, { target: { value: "5" } });
    fireEvent.blur(duration);
    expect(lastPatch()).toEqual({ durationSecs: 5 });
  });

  it("adds keyframes to each animatable property, opening the sections that have some", async () => {
    const key = [{ time_fraction: 0.5, value: 0.4 }];
    const { container } = await loaded({ width_keyframes: key });
    const sections = [...container.querySelectorAll("details")];
    // Vertices (always open for a polygon), then the five keyframe sections.
    expect(
      sections.map((d) => {
        return d.open;
      }),
    ).toEqual([true, false, false, true, false, false]);

    const adds = screen.getAllByText(t("addKeyframe"));
    expect(adds).toHaveLength(5);
    const expected = [
      { centerXKeyframes: [{ time_fraction: 1, value: 0.5 }] },
      { centerYKeyframes: [{ time_fraction: 1, value: 0.5 }] },
      { widthKeyframes: [...key, { time_fraction: 1, value: 0.4 }] },
      { heightKeyframes: [{ time_fraction: 1, value: 0.3 }] },
      { rotationKeyframes: [{ time_fraction: 1, value: 10 }] },
    ];
    adds.forEach((button, index) => {
      fireEvent.click(button);
      expect(lastPatch()).toEqual(expected[index]);
    });
  });
});
