// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { t } from "../../src/i18n";
import type { SessionView } from "../../src/types";

vi.mock("../../src/ipc", () => {
  return {
    api: {
      saveLayerTemplate: vi.fn(),
      deleteLayerTemplate: vi.fn(),
      applyLayerTemplate: vi.fn().mockResolvedValue(undefined),
    },
  };
});

const { LayerTemplates } = await import("../../src/editor/LayerTemplates");
const { api } = await import("../../src/ipc");
const { layerTemplatesStore, patchUi } = await import("../../src/store");

const session = {
  project: {
    media_library: [
      { id: 1, kind: "Video", file_name: "clip-a.mp4" },
      { id: 2, kind: "Audio", file_name: "music.mp3" },
      { id: 3, kind: "Video", file_name: "clip-b.mp4" },
    ],
  },
} as unknown as SessionView;

const template = (name: string, kinds: string[]) => {
  return {
    name,
    layers: kinds.map((kind) => {
      return [kind, {}];
    }),
  };
};

const listButton = () => {
  return screen.getByRole("button", { name: t("templates") });
};

beforeEach(() => {
  vi.clearAllMocks();
  layerTemplatesStore.set([]);
  patchUi({ selectedClipIds: [], error: null });
});
afterEach(() => {
  return cleanup();
});

describe("layer templates", () => {
  it("only offers to save a template for two or more selected clips", () => {
    render(<LayerTemplates session={session} />);
    expect((screen.getByText(t("saveAsTemplate")) as HTMLButtonElement).disabled).toBe(true);
    cleanup();

    patchUi({ selectedClipIds: [1, 2] });
    render(<LayerTemplates session={session} />);
    expect((screen.getByText(t("saveAsTemplate")) as HTMLButtonElement).disabled).toBe(false);
  });

  it("saves the selection under a trimmed name and refreshes the list", async () => {
    patchUi({ selectedClipIds: [4, 5] });
    vi.mocked(api.saveLayerTemplate).mockResolvedValue([template("Intro", ["Video"])] as never);
    render(<LayerTemplates session={session} />);
    fireEvent.click(screen.getByText(t("saveAsTemplate")));

    const confirm = screen.getByText(t("saveTemplateConfirm")) as HTMLButtonElement;
    expect(confirm.disabled).toBe(true);
    fireEvent.change(screen.getByRole("textbox"), { target: { value: "  Intro  " } });
    expect(confirm.disabled).toBe(false);
    fireEvent.click(confirm);

    expect(api.saveLayerTemplate).toHaveBeenCalledWith("Intro", [4, 5]);
    await waitFor(() => {
      return expect(layerTemplatesStore.get()).toHaveLength(1);
    });
    expect(screen.queryByText(t("saveTemplateTitle"))).toBeNull();
  });

  it("saves with Enter, ignores Enter on a blank name, and cancels", () => {
    patchUi({ selectedClipIds: [4, 5] });
    vi.mocked(api.saveLayerTemplate).mockResolvedValue(undefined as never);
    render(<LayerTemplates session={session} />);
    fireEvent.click(screen.getByText(t("saveAsTemplate")));
    const field = screen.getByRole("textbox");

    fireEvent.keyDown(field, { key: "Enter" });
    expect(api.saveLayerTemplate).not.toHaveBeenCalled();
    fireEvent.change(field, { target: { value: "Quick" } });
    fireEvent.keyDown(field, { key: "Enter" });
    expect(api.saveLayerTemplate).toHaveBeenCalledWith("Quick", [4, 5]);
    expect(screen.queryByText(t("saveTemplateTitle"))).toBeNull();

    fireEvent.click(screen.getByText(t("saveAsTemplate")));
    fireEvent.click(screen.getByText(t("cancel")));
    expect(screen.queryByText(t("saveTemplateTitle"))).toBeNull();
  });

  it("lists templates, shows the empty state and deletes one", async () => {
    render(<LayerTemplates session={session} />);
    fireEvent.click(listButton());
    expect(screen.getByText(t("noSavedTemplates"))).toBeTruthy();
    fireEvent.click(listButton());
    expect(screen.queryByText(t("noSavedTemplates"))).toBeNull();

    layerTemplatesStore.set([template("One", ["Video"]), template("Two", ["Audio"])] as never);
    vi.mocked(api.deleteLayerTemplate).mockResolvedValue([template("Two", ["Audio"])] as never);
    fireEvent.click(listButton());
    expect(await screen.findByText("One")).toBeTruthy();
    fireEvent.click(screen.getAllByText(t("deleteTemplate"))[0]);
    expect(api.deleteLayerTemplate).toHaveBeenCalledWith(0);
    await waitFor(() => {
      return expect(layerTemplatesStore.get()).toHaveLength(1);
    });
  });

  it("applies a template by choosing an asset of the right kind for each layer", () => {
    layerTemplatesStore.set([template("Duo", ["Video", "Audio"])] as never);
    render(<LayerTemplates session={session} />);
    fireEvent.click(listButton());
    fireEvent.click(screen.getByText(t("applyTemplate")));

    const apply = screen.getByText(t("applyTemplateConfirm")) as HTMLButtonElement;
    expect(apply.disabled).toBe(true);
    const [video, audio] = screen.getAllByRole("combobox") as HTMLSelectElement[];
    expect(
      [...video.options].map((o) => {
        return o.textContent;
      }),
    ).toEqual([t("applyTemplatePickAsset"), "clip-a.mp4", "clip-b.mp4"]);
    expect(
      [...audio.options].map((o) => {
        return o.textContent;
      }),
    ).toEqual([t("applyTemplatePickAsset"), "music.mp3"]);

    fireEvent.change(video, { target: { value: "3" } });
    expect(apply.disabled).toBe(false);
    fireEvent.change(audio, { target: { value: "2" } });
    fireEvent.change(audio, { target: { value: "" } });
    fireEvent.click(apply);
    expect(api.applyLayerTemplate).toHaveBeenCalledWith(0, [3, null]);
    expect(screen.queryByText(t("applyTemplateTitle"))).toBeNull();
  });

  it("closes each popup from its cancel button or the backdrop", () => {
    layerTemplatesStore.set([template("Solo", ["Video"])] as never);
    patchUi({ selectedClipIds: [1, 2] });
    const { container } = render(<LayerTemplates session={session} />);
    const overlay = () => {
      return container.querySelector(".modal-overlay")!;
    };

    fireEvent.click(listButton());
    fireEvent.click(overlay());
    expect(screen.queryByText("Solo")).toBeNull();

    fireEvent.click(listButton());
    fireEvent.click(screen.getByText(t("applyTemplate")));
    fireEvent.click(screen.getByText(t("cancel")));
    expect(screen.queryByText(t("applyTemplateTitle"))).toBeNull();

    fireEvent.click(listButton());
    fireEvent.click(screen.getByText(t("applyTemplate")));
    fireEvent.click(overlay());
    expect(screen.queryByText(t("applyTemplateTitle"))).toBeNull();

    fireEvent.click(screen.getByText(t("saveAsTemplate")));
    fireEvent.click(overlay());
    expect(screen.queryByText(t("saveTemplateTitle"))).toBeNull();
  });
});
