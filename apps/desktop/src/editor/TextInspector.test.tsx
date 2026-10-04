// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { t } from "../i18n";
import type { OverlayCatalog, TextClip } from "../types";

vi.mock("../ipc", () => ({
  api: {
    overlayCatalog: vi.fn(),
    updateTextClip: vi.fn().mockResolvedValue(undefined),
  },
}));

const { TextInspector } = await import("./TextInspector");
const { api } = await import("../ipc");

const catalog = {
  fonts: [
    { id: "inter", displayName: "Inter", category: "Sans", supportsBold: true, isVariable: true },
    { id: "mono", displayName: "Mono", category: "Monospace", supportsBold: false, isVariable: false },
  ],
  shapePresets: [],
} as unknown as OverlayCatalog;

const clip = (overrides: Partial<TextClip> = {}): TextClip => ({
  id: 5,
  start_secs: 1,
  duration_secs: 3,
  text: "Hello",
  font_size: 48,
  font_family: "inter",
  font_style: "Regular",
  font_weight: null,
  color_rgba: [255, 255, 255, 255],
  background_rgba: [0, 0, 0, 0],
  background_padding: 8,
  background_corner_radius: 4,
  direction: "Auto",
  pos_x: 0.5,
  pos_y: 0.9,
  pos_x_keyframes: [],
  pos_y_keyframes: [],
  scale_keyframes: [],
  rotation_keyframes: [],
  opacity_keyframes: [],
  words: [],
  highlight_enabled: false,
  highlight_color_rgba: [255, 255, 0, 255],
  text_align: "Auto",
  ...overrides,
});

const field = (label: string) =>
  screen.getByText(label, { selector: "label" }).parentElement as HTMLElement;
const control = (label: string, selector: string) =>
  field(label).querySelector(selector) as HTMLInputElement | HTMLSelectElement;
const lastPatch = () => vi.mocked(api.updateTextClip).mock.calls.at(-1)![1];

function slide(label: string, value: number) {
  const range = control(label, 'input[type="range"]');
  fireEvent.change(range, { target: { value: String(value) } });
  fireEvent.pointerUp(range);
}

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(api.overlayCatalog).mockResolvedValue(catalog);
});
afterEach(() => cleanup());

async function loaded(overrides: Partial<TextClip> = {}) {
  const view = render(<TextInspector clip={clip(overrides)} />);
  await waitFor(() => expect((control(t("font"), "select") as HTMLSelectElement).disabled).toBe(false));
  return view;
}

describe("text inspector", () => {
  it("keeps the font picker disabled until the catalog loads", async () => {
    render(<TextInspector clip={clip()} />);
    expect((control(t("font"), "select") as HTMLSelectElement).disabled).toBe(true);
    await waitFor(() => expect((control(t("font"), "select") as HTMLSelectElement).disabled).toBe(false));
  });

  it("commits the text content and the timing", async () => {
    await loaded();
    const text = screen.getByPlaceholderText(t("textContentHint"));
    fireEvent.change(text, { target: { value: "World" } });
    fireEvent.blur(text);
    expect(lastPatch()).toEqual({ text: "World" });
    expect(api.updateTextClip).toHaveBeenCalledWith(5, { text: "World" });

    const start = control(t("overlayStart"), "input");
    fireEvent.change(start, { target: { value: "2.5" } });
    fireEvent.blur(start);
    expect(lastPatch()).toEqual({ startSecs: 2.5 });

    const duration = control(t("overlayDuration"), "input");
    fireEvent.change(duration, { target: { value: "4" } });
    fireEvent.blur(duration);
    expect(lastPatch()).toEqual({ durationSecs: 4 });
  });

  it("groups the catalog fonts by category and flags an unknown font", async () => {
    const { container } = await loaded();
    const groups = [...container.querySelectorAll("optgroup")].map((g) => g.label);
    expect(groups).toEqual([t("fontCategorySans"), t("fontCategoryMonospace")]);
    expect(screen.queryByText(t("unknownFont"))).toBeNull();

    fireEvent.change(control(t("font"), "select"), { target: { value: "mono" } });
    expect(lastPatch()).toEqual({ fontFamily: "mono" });
    cleanup();

    await loaded({ font_family: "gone" });
    expect(screen.getByText(t("unknownFont"))).toBeTruthy();
  });

  it("enables bold only for fonts that support it and shows the weight for variable ones", async () => {
    await loaded({ font_weight: 700 });
    const style = control(t("fontStyle"), "select") as HTMLSelectElement;
    expect(style.disabled).toBe(false);
    fireEvent.change(style, { target: { value: "Bold" } });
    expect(lastPatch()).toEqual({ fontStyle: "Bold" });
    slide(t("fontWeight"), 500);
    expect(lastPatch()).toEqual({ fontWeight: 500 });
    cleanup();

    await loaded({ font_family: "mono" });
    expect((control(t("fontStyle"), "select") as HTMLSelectElement).disabled).toBe(true);
    expect(screen.queryByText(t("fontWeight"), { selector: "label" })).toBeNull();
  });

  it("defaults the variable weight to regular when the clip has none", async () => {
    await loaded();
    expect(field(t("fontWeight")).querySelector(".mono")!.textContent).toBe("400");
  });

  it("commits direction, alignment, size and color", async () => {
    await loaded();
    fireEvent.change(control(t("direction"), "select"), { target: { value: "Rtl" } });
    expect(lastPatch()).toEqual({ direction: "Rtl" });
    fireEvent.change(control(t("alignment"), "select"), { target: { value: "Center" } });
    expect(lastPatch()).toEqual({ textAlign: "Center" });
    slide(t("fontSize"), 72);
    expect(lastPatch()).toEqual({ fontSize: 72 });

    fireEvent.change(control(t("textColor"), 'input[type="color"]'), { target: { value: "#ff0000" } });
    expect(lastPatch()).toEqual({ colorRgba: [255, 0, 0, 255] });
  });

  it("toggles the background and edits it once enabled", async () => {
    await loaded();
    expect(screen.queryByText(t("backgroundColor"))).toBeNull();
    fireEvent.click(screen.getByLabelText(t("backgroundEnabled")));
    expect(lastPatch()).toEqual({ backgroundRgba: [0, 0, 0, 192] });
    cleanup();

    await loaded({ background_rgba: [10, 20, 30, 100] });
    fireEvent.change(control(t("backgroundColor"), 'input[type="color"]'), { target: { value: "#ffffff" } });
    expect(lastPatch()).toEqual({ backgroundRgba: [255, 255, 255, 100] });
    slide(t("backgroundPadding"), 20);
    expect(lastPatch()).toEqual({ backgroundPadding: 20 });
    slide(t("backgroundRadius"), 12);
    expect(lastPatch()).toEqual({ backgroundCornerRadius: 12 });

    fireEvent.click(screen.getByLabelText(t("backgroundEnabled")));
    expect(lastPatch()).toEqual({ backgroundRgba: [10, 20, 30, 0] });
  });

  it("commits the position sliders", async () => {
    await loaded();
    slide(t("positionX"), 0.25);
    expect(lastPatch()).toEqual({ posX: 0.25 });
    slide(t("positionY"), 0.75);
    expect(lastPatch()).toEqual({ posY: 0.75 });
  });

  it("opens only the keyframe sections that already have keyframes and adds new ones", async () => {
    const key = [{ time_fraction: 0.5, value: 0.4 }];
    const { container } = await loaded({ scale_keyframes: key });
    const open = [...container.querySelectorAll("details")].map((d) => d.open);
    expect(open).toEqual([false, false, true, false, false]);

    const adds = screen.getAllByText(t("addKeyframe"));
    expect(adds).toHaveLength(5);
    const expected = [
      { posXKeyframes: [{ time_fraction: 1, value: 0.5 }] },
      { posYKeyframes: [{ time_fraction: 1, value: 0.9 }] },
      { scaleKeyframes: [...key, { time_fraction: 1, value: 1 }] },
      { rotationKeyframes: [{ time_fraction: 1, value: 0 }] },
      { opacityKeyframes: [{ time_fraction: 1, value: 1 }] },
    ];
    adds.forEach((button, index) => {
      fireEvent.click(button);
      expect(lastPatch()).toEqual(expected[index]);
    });
  });

  it("offers word highlighting only for a transcribed clip", async () => {
    await loaded();
    expect(screen.queryByLabelText(t("wordHighlight"))).toBeNull();
    cleanup();

    await loaded({ words: [{}], highlight_color_rgba: [1, 2, 3, 200] });
    fireEvent.click(screen.getByLabelText(t("wordHighlight")));
    expect(lastPatch()).toEqual({ highlightEnabled: true });
    fireEvent.change(control(t("highlightColor"), 'input[type="color"]'), { target: { value: "#0a0b0c" } });
    expect(lastPatch()).toEqual({ highlightColorRgba: [10, 11, 12, 200] });
  });
});
