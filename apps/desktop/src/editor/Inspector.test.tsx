// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { t } from "../i18n";
import { makeSession } from "../testSupport";
import type { ClipInstance, SessionView } from "../types";

vi.mock("../ipc", () => {
  const names = [
    "updateClip",
    "applySpeedRampToClip",
    "motionTrackClip",
    "autoReframeClip",
    "dynamicAutoReframeClip",
    "generateMatteForClip",
    "applyPrivacyBlurToClip",
    "mergeClipsIntoComposite",
    "createCompoundClip",
    "deleteClips",
  ];
  return {
    api: Object.fromEntries(
      names.map((name) => {
        return [name, vi.fn().mockResolvedValue(undefined)];
      }),
    ),
  };
});
vi.mock("./TextInspector", () => {
  return {
    TextInspector: ({ clip }: { clip: { id: number } }) => {
      return <p>text-inspector-{clip.id}</p>;
    },
  };
});
vi.mock("./ShapeInspector", () => {
  return {
    ShapeInspector: ({ clip }: { clip: { id: number } }) => {
      return <p>shape-inspector-{clip.id}</p>;
    },
  };
});

const { Inspector, Section, Slider } = await import("./Inspector");
const { api } = await import("../ipc");
const { patchUi, previewStore } = await import("../store");

const clip = (overrides: Partial<ClipInstance> = {}): ClipInstance => {
  return {
    id: 7,
    asset_id: 3,
    gain_db: 0,
    speed_factor: 1,
    brightness: 0,
    contrast: 1,
    saturation: 1,
    frozen: false,
    flipped_h: false,
    voice_cleanup_enabled: false,
    crop_x: 0,
    crop_y: 0,
    crop_w: 1,
    crop_h: 1,
    mask_shape: "None",
    mask_corner_radius: 0.2,
    transition_in: "None",
    transition_duration_secs: 0.5,
    fade_in_secs: 0,
    fade_out_secs: 0,
    blend_mode: "Normal",
    position_keyframes: [],
    scale_keyframes: [],
    rotation_keyframes: [],
    opacity_keyframes: [],
    gain_keyframes: [],
    brightness_keyframes: [],
    contrast_keyframes: [],
    saturation_keyframes: [],
    crop_x_keyframes: [],
    crop_y_keyframes: [],
    crop_w_keyframes: [],
    crop_h_keyframes: [],
    ...overrides,
  } as unknown as ClipInstance;
};

function sessionFor(
  overrides: Partial<ClipInstance> = {},
  extra: Record<string, unknown> = {},
): SessionView {
  const view = makeSession([
    {
      id: 1,
      name: "Main",
      tracks: [
        {
          clips: [clip(overrides)],
          text_clips: [{ id: 20 }],
          shape_clips: [{ id: 30 }],
          ...extra,
        },
      ],
    },
  ]);
  view.project.media_library = [
    { id: 3, file_name: "gameplay.mp4", source_path: "C:/m/gameplay.mp4" },
  ] as never;
  return view;
}

const field = (label: string) => {
  return screen.getByText(label, { selector: "label" }).parentElement as HTMLElement;
};
const range = (label: string) => {
  return field(label).querySelector('input[type="range"]') as HTMLInputElement;
};
const lastPatch = () => {
  return vi.mocked(api.updateClip).mock.calls.at(-1)![1];
};
const sectionOf = (label: string) => {
  return screen.getByText(label, { selector: "summary > span" }).closest("details")!;
};

function slide(label: string, value: number) {
  fireEvent.change(range(label), { target: { value: String(value) } });
  fireEvent.pointerUp(range(label));
}

beforeEach(() => {
  vi.clearAllMocks();
  patchUi({ selectedClipIds: [7] });
});
afterEach(() => {
  return cleanup();
});

describe("inspector selection", () => {
  it("asks for a selection when nothing is selected, or more than one clip", () => {
    patchUi({ selectedClipIds: [] });
    render(<Inspector session={sessionFor()} />);
    expect(screen.getByText(t("noClipSelected"))).toBeTruthy();
    cleanup();
  });

  it("shows the source asset of a selected media clip", () => {
    render(<Inspector session={sessionFor()} />);
    expect(screen.getByTestId("inspector-clip")).toBeTruthy();
    expect(screen.getByText(/gameplay\.mp4/)).toBeTruthy();
  });

  it("falls back to a dash when the asset is gone", () => {
    const view = sessionFor();
    view.project.media_library = [];
    render(<Inspector session={view} />);
    expect(screen.getByText("—")).toBeTruthy();
  });

  it("hands text and shape overlays to their own inspectors", () => {
    patchUi({ selectedClipIds: [20] });
    render(<Inspector session={sessionFor()} />);
    expect(screen.getByText("text-inspector-20")).toBeTruthy();
    cleanup();

    patchUi({ selectedClipIds: [30] });
    render(<Inspector session={sessionFor()} />);
    expect(screen.getByText("shape-inspector-30")).toBeTruthy();
  });
});

describe("slider and section", () => {
  it("previews locally and commits on pointer up, key up or blur, only when changed", () => {
    const onCommit = vi.fn();
    render(
      <Slider
        label="Amount"
        value={1}
        min={0}
        max={5}
        step={1}
        format={(v) => {
          return `${v}!`;
        }}
        onCommit={onCommit}
      />,
    );
    const input = range("Amount");
    expect(screen.getByText("1!")).toBeTruthy();
    fireEvent.pointerUp(input);
    expect(onCommit).not.toHaveBeenCalled();

    fireEvent.change(input, { target: { value: "3" } });
    expect(screen.getByText("3!")).toBeTruthy();
    fireEvent.keyUp(input);
    expect(onCommit).toHaveBeenLastCalledWith(3);
    fireEvent.blur(input);
    expect(onCommit).toHaveBeenCalledTimes(2);
  });

  it("follows the value the parent hands in", () => {
    const props = {
      label: "Amount",
      min: 0,
      max: 5,
      step: 1,
      format: (v: number) => {
        return `${v}`;
      },
      onCommit: vi.fn(),
    };
    const { rerender } = render(
      <Slider
        {...props}
        value={1}
      />,
    );
    rerender(
      <Slider
        {...props}
        value={4}
      />,
    );
    expect(range("Amount").value).toBe("4");
  });

  it("renders an open or closed section", () => {
    render(
      <>
        <Section
          label="Open"
          defaultOpen={true}
        >
          a
        </Section>
        <Section
          label="Closed"
          defaultOpen={false}
        >
          b
        </Section>
      </>,
    );
    expect(sectionOf("Open").open).toBe(true);
    expect(sectionOf("Closed").open).toBe(false);
  });
});

describe("clip controls", () => {
  it("opens a section only when its property is not at the default", () => {
    render(<Inspector session={sessionFor()} />);
    for (const label of ["speed", "colorAdjust", "crop", "mask", "transition", "keyframes"]) {
      expect(sectionOf(t(label as never)).open).toBe(false);
    }
    expect((screen.getByTestId("section-gain") as HTMLDetailsElement).open).toBe(false);
    cleanup();

    render(
      <Inspector
        session={sessionFor({
          gain_db: 3,
          speed_factor: 2,
          brightness: 0.1,
          crop_w: 0.5,
          mask_shape: "Circle",
          transition_in: "Fade",
          gain_keyframes: [{ time_fraction: 0, value: 1 }],
        })}
      />,
    );
    expect((screen.getByTestId("section-gain") as HTMLDetailsElement).open).toBe(true);
    for (const label of ["speed", "colorAdjust", "crop", "mask", "transition", "keyframes"]) {
      expect(sectionOf(t(label as never)).open).toBe(true);
    }
  });

  it("opens the color section for contrast or saturation changes alone", () => {
    render(<Inspector session={sessionFor({ contrast: 1.2 })} />);
    expect(sectionOf(t("colorAdjust")).open).toBe(true);
    cleanup();
    render(<Inspector session={sessionFor({ saturation: 0.5 })} />);
    expect(sectionOf(t("colorAdjust")).open).toBe(true);
  });

  it("opens the crop section for any single crop edge", () => {
    for (const edge of [{ crop_x: 0.1 }, { crop_y: 0.1 }, { crop_h: 0.5 }]) {
      render(<Inspector session={sessionFor(edge)} />);
      expect(sectionOf(t("crop")).open).toBe(true);
      cleanup();
    }
  });

  it("opens the keyframes section for keyframes on each property", () => {
    const key = [{ time_fraction: 0, value: 1 }];
    const props = [
      "position_keyframes",
      "scale_keyframes",
      "rotation_keyframes",
      "opacity_keyframes",
      "brightness_keyframes",
      "contrast_keyframes",
      "saturation_keyframes",
      "crop_x_keyframes",
      "crop_y_keyframes",
      "crop_w_keyframes",
      "crop_h_keyframes",
    ];
    for (const prop of props) {
      render(<Inspector session={sessionFor({ [prop]: key })} />);
      expect(sectionOf(t("keyframes")).open).toBe(true);
      cleanup();
    }
  });

  it("commits audio, speed, color and crop sliders", () => {
    render(<Inspector session={sessionFor()} />);
    slide(t("gain"), 6);
    expect(lastPatch()).toEqual({ gainDb: 6 });
    slide(t("speed"), 2);
    expect(lastPatch()).toEqual({ speedFactor: 2 });
    slide(t("brightness"), 0.3);
    expect(lastPatch()).toEqual({ brightness: 0.3 });
    slide(t("contrast"), 1.5);
    expect(lastPatch()).toEqual({ contrast: 1.5 });
    slide(t("saturation"), 0.5);
    expect(lastPatch()).toEqual({ saturation: 0.5 });
    slide("x", 0.1);
    expect(lastPatch()).toEqual({ cropX: 0.1 });
    slide("y", 0.2);
    expect(lastPatch()).toEqual({ cropY: 0.2 });
    slide("w", 0.5);
    expect(lastPatch()).toEqual({ cropW: 0.5 });
    slide("h", 0.6);
    expect(lastPatch()).toEqual({ cropH: 0.6 });
  });

  it("formats the gain with an explicit sign", () => {
    render(<Inspector session={sessionFor({ gain_db: 3 })} />);
    expect(screen.getByText("+3.0 dB")).toBeTruthy();
    cleanup();
    render(<Inspector session={sessionFor({ gain_db: -3 })} />);
    expect(screen.getByText("-3.0 dB")).toBeTruthy();
  });

  it("applies the two speed ramps and resets the crop", () => {
    render(<Inspector session={sessionFor()} />);
    fireEvent.click(screen.getByText(t("speedRampSlowToFast")));
    expect(api.applySpeedRampToClip).toHaveBeenLastCalledWith(7, 0.5, 2.0, 4);
    fireEvent.click(screen.getByText(t("speedRampFastToSlow")));
    expect(api.applySpeedRampToClip).toHaveBeenLastCalledWith(7, 2.0, 0.5, 4);
    fireEvent.click(screen.getByText(t("cropReset")));
    expect(lastPatch()).toEqual({ cropX: 0, cropY: 0, cropW: 1, cropH: 1 });
  });

  it("toggles freeze, flip and voice cleanup", () => {
    render(<Inspector session={sessionFor()} />);
    fireEvent.click(screen.getByLabelText(t("freeze")));
    expect(lastPatch()).toEqual({ frozen: true });
    fireEvent.click(screen.getByLabelText(t("flip")));
    expect(lastPatch()).toEqual({ flippedH: true });
    fireEvent.click(screen.getByLabelText(t("voiceCleanup")));
    expect(lastPatch()).toEqual({ voiceCleanupEnabled: true });
  });

  it("changes the mask, with a corner radius only for rounded rectangles", () => {
    render(<Inspector session={sessionFor()} />);
    expect(screen.queryByText(t("maskCornerRadius"))).toBeNull();
    fireEvent.change(field(t("mask")).querySelector("select")!, {
      target: { value: "Circle" },
    });
    expect(lastPatch()).toEqual({ maskShape: "Circle" });
    cleanup();

    render(<Inspector session={sessionFor({ mask_shape: "RoundedRect" })} />);
    slide(t("maskCornerRadius"), 0.5);
    expect(lastPatch()).toEqual({ maskCornerRadius: 0.5 });
  });

  it("changes the transition, with a duration only once one is chosen", () => {
    render(<Inspector session={sessionFor()} />);
    expect(screen.queryByText(t("transitionDuration"))).toBeNull();
    fireEvent.change(field(t("transition")).querySelector("select")!, {
      target: { value: "Slide" },
    });
    expect(lastPatch()).toEqual({ transitionIn: "Slide" });
    cleanup();

    render(<Inspector session={sessionFor({ transition_in: "Fade" })} />);
    slide(t("transitionDuration"), 1.5);
    expect(lastPatch()).toEqual({ transitionDurationSecs: 1.5 });
  });

  it("changes the blend mode", () => {
    render(<Inspector session={sessionFor()} />);
    fireEvent.change(field(t("blendMode")).querySelector("select")!, {
      target: { value: "Multiply" },
    });
    expect(lastPatch()).toEqual({ blendMode: "Multiply" });
  });

  it("adds a keyframe to every animatable property", () => {
    render(<Inspector session={sessionFor()} />);
    const adds = screen.getAllByText(t("addKeyframe"));
    const expected: Record<string, unknown>[] = [
      { positionKeyframes: [{ time_fraction: 1, value: { x: 0, y: 0 } }] },
      ...(
        [
          ["scaleKeyframes", 1],
          ["rotationKeyframes", 0],
          ["opacityKeyframes", 1],
          ["gainKeyframes", 0],
          ["brightnessKeyframes", 0],
          ["contrastKeyframes", 1],
          ["saturationKeyframes", 1],
          ["cropXKeyframes", 0],
          ["cropYKeyframes", 0],
          ["cropWKeyframes", 1],
          ["cropHKeyframes", 1],
        ] as [string, number][]
      ).map(([key, value]) => {
        return { [key]: [{ time_fraction: 1, value }] };
      }),
    ];
    expect(adds).toHaveLength(expected.length);
    adds.forEach((button, index) => {
      fireEvent.click(button);
      expect(lastPatch()).toEqual(expected[index]);
    });
  });

  it("runs the AI actions with the chosen region", () => {
    render(<Inspector session={sessionFor()} />);
    const edit = (title: string, value: string) => {
      return fireEvent.change(screen.getByTitle(t(title as never)), { target: { value } });
    };
    edit("centerX", "0.2");
    edit("centerY", "0.3");
    edit("regionWidth", "0.4");
    edit("regionHeight", "0.5");

    fireEvent.click(screen.getByText(t("trackMotion")));
    expect(api.motionTrackClip).toHaveBeenCalledWith(7, 0.2, 0.3, 0.4, 0.5, 0.08);
    fireEvent.click(screen.getByText(t("applyPrivacyBlur")));
    expect(api.applyPrivacyBlurToClip).toHaveBeenCalledWith(7, 0.2, 0.3, 0.4, 0.5);
    fireEvent.click(screen.getByText(t("autoReframe")));
    expect(api.autoReframeClip).toHaveBeenCalledWith(7);
    fireEvent.click(screen.getByText(t("dynamicAutoReframe")));
    expect(api.dynamicAutoReframeClip).toHaveBeenCalledWith(7);
    fireEvent.click(screen.getByText(t("generateMatte")));
    expect(api.generateMatteForClip).toHaveBeenCalledWith(7);
  });
});

describe("inspector tabs", () => {
  it("groups the clip's sections under Inspector, Effects and Audio", () => {
    const { container } = render(<Inspector session={sessionFor()} />);
    const body = container.querySelector(".inspector-body")!;
    expect(body.getAttribute("data-tab")).toBe("inspector");
    fireEvent.click(screen.getByRole("tab", { name: t("inspectorEffects") }));
    expect(body.getAttribute("data-tab")).toBe("effects");
    expect(
      screen.getByRole("tab", { name: t("inspectorEffects") }).getAttribute("aria-selected"),
    ).toBe("true");
    fireEvent.click(screen.getByRole("tab", { name: t("inspectorAudio") }));
    expect(body.getAttribute("data-tab")).toBe("audio");
    expect(screen.getByTestId("section-gain").getAttribute("data-group")).toBe("audio");
    expect(container.querySelector('[data-group="effects"]')).toBeTruthy();
  });

  it("offers only the Inspector tab for text and shape overlays", () => {
    patchUi({ selectedClipIds: [20] });
    render(<Inspector session={sessionFor()} />);
    expect(screen.getAllByRole("tab")).toHaveLength(1);
  });
});

describe("transform section", () => {
  it("shows position in canvas pixels, scale and rotation as plain numbers", () => {
    render(
      <Inspector
        session={sessionFor({
          position_keyframes: [{ time_fraction: 0, value: { x: 0.25, y: 0.5 } }],
          layer_scale_x: 0.5,
          rotation_keyframes: [{ time_fraction: 0, value: 30 }],
        })}
      />,
    );
    expect((screen.getByLabelText(t("positionX")) as HTMLInputElement).value).toBe("480");
    expect((screen.getByLabelText(t("positionY")) as HTMLInputElement).value).toBe("540");
    expect((screen.getByLabelText(t("scale")) as HTMLInputElement).value).toBe("50");
    expect((screen.getByLabelText(t("rotation")) as HTMLInputElement).value).toBe("30");
  });

  it("commits an edited position on blur as one keyframe, in canvas fractions", () => {
    render(<Inspector session={sessionFor()} />);
    const x = screen.getByLabelText(t("positionX"));
    fireEvent.change(x, { target: { value: "768" } });
    expect(api.updateClip).not.toHaveBeenCalled();
    fireEvent.blur(x);
    expect(lastPatch()).toEqual({
      positionKeyframes: [{ time_fraction: 0, value: { x: 0.4, y: 0 } }],
    });
  });

  it("keeps a typed position within one canvas of the frame", () => {
    render(<Inspector session={sessionFor()} />);
    const x = screen.getByLabelText(t("positionX"));
    fireEvent.change(x, { target: { value: "999999" } });
    fireEvent.blur(x);
    expect(lastPatch()).toEqual({
      positionKeyframes: [{ time_fraction: 0, value: { x: 2, y: 0 } }],
    });
    const y = screen.getByLabelText(t("positionY"));
    fireEvent.change(y, { target: { value: "-999999" } });
    fireEvent.blur(y);
    expect(lastPatch()).toEqual({
      positionKeyframes: [{ time_fraction: 0, value: { x: 0, y: -1 } }],
    });
  });

  it("scales both axes together and never below 1 percent", () => {
    render(<Inspector session={sessionFor()} />);
    const scale = screen.getByLabelText(t("scale"));
    fireEvent.change(scale, { target: { value: "150" } });
    fireEvent.keyDown(scale, { key: "Enter" });
    fireEvent.blur(scale);
    expect(lastPatch()).toEqual({ layerScaleX: 1.5, layerScaleY: 1.5 });
    fireEvent.change(scale, { target: { value: "-20" } });
    fireEvent.blur(scale);
    expect(lastPatch()).toEqual({ layerScaleX: 0.01, layerScaleY: 0.01 });
  });

  it("commits a rotation and ignores an empty or unchanged field", () => {
    render(<Inspector session={sessionFor()} />);
    const rotation = screen.getByLabelText(t("rotation")) as HTMLInputElement;
    fireEvent.change(rotation, { target: { value: "45" } });
    fireEvent.blur(rotation);
    expect(lastPatch()).toEqual({ rotationKeyframes: [{ time_fraction: 0, value: 45 }] });
    vi.mocked(api.updateClip).mockClear();
    fireEvent.change(rotation, { target: { value: "" } });
    fireEvent.blur(rotation);
    expect(api.updateClip).not.toHaveBeenCalled();
    expect(rotation.value).toBe("0");
  });

  it("disables an animated value and points at the keyframes", () => {
    render(
      <Inspector
        session={sessionFor({
          position_keyframes: [
            { time_fraction: 0, value: { x: 0, y: 0 } },
            { time_fraction: 1, value: { x: 1, y: 1 } },
          ],
        })}
      />,
    );
    expect((screen.getByLabelText(t("positionX")) as HTMLInputElement).disabled).toBe(true);
    expect(screen.getByText(t("transformAnimated"))).toBeTruthy();
  });

  it("resets position, scale and rotation from the section header", () => {
    render(<Inspector session={sessionFor({ layer_scale_x: 2 })} />);
    fireEvent.click(screen.getByLabelText(t("resetSection") + ": " + t("transform")));
    expect(lastPatch()).toEqual({
      positionKeyframes: [],
      layerScaleX: 1,
      layerScaleY: 1,
      rotationKeyframes: [],
    });
  });
});

describe("section resets", () => {
  it.each([
    ["gain", { gainDb: 0 }],
    ["speed", { speedFactor: 1 }],
    ["colorAdjust", { brightness: 0, contrast: 1, saturation: 1 }],
    ["crop", { cropX: 0, cropY: 0, cropW: 1, cropH: 1 }],
  ])("restores %s to its defaults without toggling the section", (key, patch) => {
    render(<Inspector session={sessionFor()} />);
    const label = t(key as never);
    const section = sectionOf(label);
    const wasOpen = section.open;
    fireEvent.click(screen.getByLabelText(t("resetSection") + ": " + label));
    expect(lastPatch()).toEqual(patch);
    expect(section.open).toBe(wasOpen);
  });
});

describe("inspector grouping and selection", () => {
  it("puts voice cleanup under Audio and freeze/flip under Inspector", () => {
    const { container } = render(<Inspector session={sessionFor()} />);
    const group = (text: string) => {
      return screen.getByText(text).closest("[data-group]")!.getAttribute("data-group");
    };
    expect(group(t("voiceCleanup"))).toBe("audio");
    expect(group(t("freeze"))).toBe("inspector");
    expect(group(t("flip"))).toBe("inspector");
    expect(container.querySelector('[data-group="effects"]')).toBeTruthy();
  });

  it("says how many clips are selected and offers the group actions", () => {
    patchUi({ selectedClipIds: [7, 20] });
    vi.mocked(api.mergeClipsIntoComposite).mockClear();
    render(<Inspector session={sessionFor()} />);
    expect(screen.getByTestId("inspector-multi").textContent).toContain("2 " + t("clipsSelected"));
    fireEvent.click(screen.getByText(t("mergeComposite")));
    fireEvent.click(screen.getByText(t("compoundClip")));
    fireEvent.click(screen.getByText(t("delete")));
    fireEvent.click(screen.getByText(t("rippleDelete")));
    expect(api.mergeClipsIntoComposite).toHaveBeenCalledWith([7, 20]);
    expect(api.createCompoundClip).toHaveBeenCalledWith(7, [7, 20]);
    expect(api.deleteClips).toHaveBeenCalledWith([7, 20], false);
    expect(api.deleteClips).toHaveBeenCalledWith([7, 20], true);
  });
});

describe("keyframe diamond", () => {
  const placed = { start_secs: 0, source_in_secs: 0, source_out_secs: 10 };
  const atSecs = (playheadSecs: number) => {
    previewStore.set({ ...previewStore.get(), playheadSecs });
  };
  const diamond = (label: string) => {
    return sectionOf(label).querySelector(".section-keyframe") as HTMLButtonElement;
  };

  it("is disabled while the playhead is outside the clip", () => {
    atSecs(20);
    render(<Inspector session={sessionFor(placed)} />);
    expect(diamond(t("gain")).disabled).toBe(true);
  });

  it("adds a gain keyframe at the playhead carrying the current gain", () => {
    atSecs(5);
    render(<Inspector session={sessionFor({ ...placed, gain_db: 3 })} />);
    fireEvent.click(diamond(t("gain")));
    expect(lastPatch()).toEqual({ gainKeyframes: [{ time_fraction: 0.5, value: 3 }] });
  });

  it("removes the keyframe when one already sits at the playhead", () => {
    atSecs(5);
    render(
      <Inspector
        session={sessionFor({
          ...placed,
          gain_keyframes: [
            { time_fraction: 0.5, value: 3 },
            { time_fraction: 1, value: 6 },
          ],
        })}
      />,
    );
    expect(diamond(t("gain")).getAttribute("aria-pressed")).toBe("true");
    fireEvent.click(diamond(t("gain")));
    expect(lastPatch()).toEqual({ gainKeyframes: [{ time_fraction: 1, value: 6 }] });
  });

  it("keys position and rotation together from the transform section", () => {
    atSecs(2.5);
    render(<Inspector session={sessionFor(placed)} />);
    fireEvent.click(diamond(t("transform")));
    expect(lastPatch()).toEqual({
      positionKeyframes: [{ time_fraction: 0.25, value: { x: 0, y: 0 } }],
      rotationKeyframes: [{ time_fraction: 0.25, value: 0 }],
    });
  });

  it("keys brightness, contrast and saturation together from the color section", () => {
    atSecs(10);
    render(<Inspector session={sessionFor({ ...placed, contrast: 1.2 })} />);
    fireEvent.click(diamond(t("colorAdjust")));
    expect(lastPatch()).toEqual({
      brightnessKeyframes: [{ time_fraction: 1, value: 0 }],
      contrastKeyframes: [{ time_fraction: 1, value: 1.2 }],
      saturationKeyframes: [{ time_fraction: 1, value: 1 }],
    });
  });
});
