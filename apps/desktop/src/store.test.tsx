// @vitest-environment jsdom
import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createStore, describeError, patchUi, run, uiStore, useStore } from "./store";
import { t } from "./i18n";

afterEach(() => {
  return cleanup();
});

describe("createStore", () => {
  it("holds a value, replaces it and notifies subscribers once per change", () => {
    const store = createStore(1);
    const listener = vi.fn();
    const unsubscribe = store.subscribe(listener);

    store.set(2);
    expect(store.get()).toBe(2);
    store.set((previous) => {
      return previous + 3;
    });
    expect(store.get()).toBe(5);
    expect(listener).toHaveBeenCalledTimes(2);

    // Setting the same value is not a change.
    store.set(5);
    store.set((previous) => {
      return previous;
    });
    expect(listener).toHaveBeenCalledTimes(2);

    unsubscribe();
    store.set(9);
    expect(listener).toHaveBeenCalledTimes(2);
  });

  it("re-renders a component reading it through useStore", () => {
    const store = createStore("a");
    const { result } = renderHook(() => {
      return useStore(store);
    });
    expect(result.current).toBe("a");
    act(() => {
      return store.set("b");
    });
    expect(result.current).toBe("b");
  });
});

describe("ui state helpers", () => {
  beforeEach(() => {
    return patchUi({ error: null, screen: "home", selectedClipIds: [] });
  });

  it("patchUi merges into the transient state", () => {
    patchUi({ screen: "editor", selectedClipIds: [3, 4] });
    expect(uiStore.get().screen).toBe("editor");
    expect(uiStore.get().selectedClipIds).toEqual([3, 4]);
    expect(uiStore.get().tool).toBe("select");
  });

  it("run returns the value of a call that succeeds", async () => {
    await expect(run(Promise.resolve(7))).resolves.toBe(7);
    expect(uiStore.get().error).toBeNull();
  });

  it("run shows a rejection in the error banner instead of throwing", async () => {
    await expect(run(Promise.reject(new Error("backend down")))).resolves.toBeUndefined();
    expect(uiStore.get().error).toContain("backend down");
  });
});

describe("describeError", () => {
  it("translates the codes the backend uses instead of a sentence", () => {
    expect(describeError("assetInUse")).toBe(t("assetInUse"));
    expect(describeError("pathMissing")).toBe(t("pathMissing"));
    expect(describeError("outputIsSource")).toBe(t("exportOutputIsSource"));
    expect(describeError("outputInQueue")).toBe(t("exportOutputInQueue"));
    expect(describeError("projectMissing")).toBe(t("projectMissing"));
  });

  it("passes any other error through as the backend wrote it", () => {
    expect(describeError("clip not found")).toBe("clip not found");
    expect(describeError(new Error("boom"))).toContain("boom");
  });

  it("shows a translated code in the banner when a call fails with it", async () => {
    await run(Promise.reject("assetInUse"));
    expect(uiStore.get().error).toBe(t("assetInUse"));
  });
});

describe("connect", () => {
  type Handler = (payload: never) => void;
  const handlers = new Map<string, Handler>();

  function session(activeId: number, clipIds: number[]) {
    return {
      project: {
        active_sequence: 0,
        sequences: [
          {
            id: activeId,
            timeline: {
              tracks: [
                {
                  clips: clipIds.map((id) => {
                    return { id };
                  }),
                },
              ],
            },
          },
        ],
      },
    };
  }

  async function load() {
    vi.resetModules();
    handlers.clear();
    const subscribe = (name: string) => {
      return async (handler: Handler) => {
        handlers.set(name, handler);
        return () => {
          return undefined;
        };
      };
    };
    const api = {
      getSession: vi.fn().mockResolvedValue(session(1, [1, 2, 3])),
      getPrefs: vi.fn().mockResolvedValue({ locale: "en", savedLayerTemplates: [{ name: "t" }] }),
      listExports: vi.fn().mockResolvedValue([{ id: 9 }]),
    };
    vi.doMock("./ipc", () => {
      return {
        api,
        onSession: subscribe("session"),
        onPreview: subscribe("preview"),
        onExports: subscribe("exports"),
        onImportFailed: subscribe("importFailed"),
        onPreviewRecovery: subscribe("previewRecovery"),
        onSceneCut: subscribe("sceneCut"),
        onMotionTrack: subscribe("motionTrack"),
        onAutoReframe: subscribe("autoReframe"),
        onMatteGeneration: subscribe("matte"),
        onPrivacyBlur: subscribe("privacy"),
        onTts: subscribe("tts"),
        onTranscribe: subscribe("transcribe"),
      };
    });
    const store = await import("./store");
    const i18n = await import("./i18n");
    await store.connect();
    return { store, i18n, api };
  }

  afterEach(() => {
    return vi.doUnmock("./ipc");
  });

  it("loads the initial session, language, templates and exports once", async () => {
    const { store, i18n, api } = await load();
    expect(store.sessionStore.get()).toEqual(session(1, [1, 2, 3]));
    expect(i18n.localeSnapshot()).toBe("en");
    expect(store.layerTemplatesStore.get()).toEqual([{ name: "t" }]);
    expect(store.exportsStore.get()).toEqual([{ id: 9 }]);

    // A second connect does nothing.
    await store.connect();
    expect(api.getSession).toHaveBeenCalledTimes(1);
    i18n.setLocale("pt-BR");
  });

  it("tells the user which file could not be imported", async () => {
    const { store, i18n } = await load();
    handlers.get("importFailed")!("broken.mp4" as never);
    expect(store.uiStore.get().error).toBe(i18n.t("importFailed") + ": broken.mp4");
  });

  it("tells the user when playback crashed and whether it came back", async () => {
    const { store, i18n } = await load();
    handlers.get("previewRecovery")!(true as never);
    expect(store.uiStore.get().error).toBe(i18n.t("previewRestarted"));
    handlers.get("previewRecovery")!(false as never);
    expect(store.uiStore.get().error).toBe(i18n.t("previewGaveUp"));
  });

  it("forwards preview ticks and export lists to their stores", async () => {
    const { store } = await load();
    handlers.get("preview")!({ frameSeq: 4, playheadSecs: 2 } as never);
    expect(store.previewStore.get().frameSeq).toBe(4);
    handlers.get("exports")!([{ id: 1 }, { id: 2 }] as never);
    expect(store.exportsStore.get()).toHaveLength(2);
  });

  it("drops selections that no longer exist and clears them when the sequence changes", async () => {
    const { store } = await load();
    store.patchUi({ selectedClipIds: [1, 99] });
    handlers.get("session")!(session(1, [1, 2]) as never);
    expect(store.uiStore.get().selectedClipIds).toEqual([1]);

    store.patchUi({ selectedClipIds: [1] });
    handlers.get("session")!(session(2, [1]) as never);
    expect(store.uiStore.get().selectedClipIds).toEqual([]);

    // Nothing to drop when the selection is still valid.
    store.patchUi({ selectedClipIds: [1] });
    handlers.get("session")!(session(2, [1, 5]) as never);
    expect(store.uiStore.get().selectedClipIds).toEqual([1]);
  });

  it("reports analysis events that found nothing or failed", async () => {
    const { store, i18n } = await load();
    const banner = () => {
      return store.uiStore.get().error;
    };
    const fire = (name: string, payload: unknown) => {
      return handlers.get(name)!(payload as never);
    };

    fire("sceneCut", 3);
    expect(banner()).toBeNull();
    fire("sceneCut", 0);
    expect(banner()).toBe(i18n.t("sceneCutDetectionNone"));

    store.patchUi({ error: null });
    fire("motionTrack", 0);
    expect(banner()).toBe(i18n.t("motionTrackNoFramesDecoded"));

    store.patchUi({ error: null });
    fire("autoReframe", { subjectFound: true });
    expect(banner()).toBeNull();
    fire("autoReframe", { subjectFound: false });
    expect(banner()).toBe(i18n.t("autoReframeNoSubjectFound"));

    for (const name of ["matte", "privacy", "tts"]) {
      store.patchUi({ error: null });
      fire(name, { type: "done" });
      expect(banner(), name).toBeNull();
      fire(name, { type: "failed", message: `${name} failed` });
      expect(banner(), name).toBe(`${name} failed`);
    }
  });

  it("tracks which asset is being transcribed and reports its outcome", async () => {
    const { store, i18n } = await load();
    const fire = (payload: unknown) => {
      return handlers.get("transcribe")!(payload as never);
    };

    fire({ type: "started", assetId: 12 });
    expect(store.transcribingAssetIdStore.get()).toBe(12);

    fire({ type: "failed", message: "model missing" });
    expect(store.transcribingAssetIdStore.get()).toBeNull();
    expect(store.uiStore.get().error).toBe("model missing");

    store.patchUi({ error: null });
    fire({ type: "started", assetId: 12 });
    fire({ type: "noSpeech" });
    expect(store.transcribingAssetIdStore.get()).toBeNull();
    expect(store.uiStore.get().error).toBe(i18n.t("transcribeNoSpeechFound"));

    store.patchUi({ error: null });
    fire({ type: "done" });
    expect(store.uiStore.get().error).toBeNull();
  });
});
