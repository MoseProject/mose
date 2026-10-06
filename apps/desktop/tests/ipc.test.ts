// @vitest-environment jsdom
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.fn();
const listeners = new Map<string, (event: { payload: unknown }) => void>();
const unlisten = vi.fn();
const listen = vi.fn(async (name: string, handler: (event: { payload: unknown }) => void) => {
  listeners.set(name, handler);
  return unlisten;
});

vi.mock("@tauri-apps/api/core", () => {
  return {
    invoke: (...args: unknown[]) => {
      return invoke(...args);
    },
  };
});
vi.mock("@tauri-apps/api/event", () => {
  return {
    listen: (...args: Parameters<typeof listen>) => {
      return listen(...args);
    },
  };
});

const ipc = await import("../src/ipc");

/** Every command name registered with `generate_handler!` in the Rust shell. */
function registeredCommands(): Set<string> {
  const main = readFileSync(resolve(__dirname, "../src-tauri/src/handler"), "utf8");
  const block = main.slice(main.indexOf("generate_handler!["));
  const body = block.slice(0, block.indexOf("])"));
  const names = [...body.matchAll(/\$crate::(?:\w+::)*(\w+)/g)].map((match) => {
    return match[1];
  });
  return new Set(names);
}

beforeEach(() => {
  invoke.mockReset();
  invoke.mockResolvedValue(undefined);
  listen.mockClear();
  listeners.clear();
});

describe("api wrappers", () => {
  it("only invoke commands the Rust shell registers", async () => {
    const registered = registeredCommands();
    expect(registered.size).toBeGreaterThan(50);
    const unknown: string[] = [];
    for (const [name, call] of Object.entries(ipc.api)) {
      invoke.mockClear();
      // The wrappers only forward their arguments, so placeholder values are enough.
      await (call as (...args: unknown[]) => Promise<unknown>)(1, 2, 3, 4, 5, 6);
      expect(invoke, name).toHaveBeenCalledTimes(1);
      const command = invoke.mock.calls[0][0] as string;
      expect(command, name).toMatch(/^[a-z][a-z0-9_]*$/);
      if (!registered.has(command)) {
        unknown.push(`${name} -> ${command}`);
      }
    }
    expect(unknown).toEqual([]);
  });

  it("forward their arguments under camelCase keys and fill optional ones with null", async () => {
    await ipc.api.moveClip(7, 3, 12.5);
    expect(invoke).toHaveBeenLastCalledWith("move_clip", {
      clipId: 7,
      trackId: 3,
      startSecs: 12.5,
    });

    await ipc.api.addAssetToTimeline(4);
    expect(invoke).toHaveBeenLastCalledWith("add_asset_to_timeline", {
      assetId: 4,
      trackId: null,
      startSecs: null,
    });
    await ipc.api.addAssetToTimeline(4, 2, 9);
    expect(invoke).toHaveBeenLastCalledWith("add_asset_to_timeline", {
      assetId: 4,
      trackId: 2,
      startSecs: 9,
    });

    await ipc.api.addMarker();
    expect(invoke).toHaveBeenLastCalledWith("add_marker", { kind: "Standard" });
    await ipc.api.updateMarker(5, { label: "x" });
    expect(invoke).toHaveBeenLastCalledWith("update_marker", {
      markerId: 5,
      label: "x",
      kind: null,
      completed: null,
    });
    await ipc.api.updateMarker(5, { kind: "Chapter", completed: true });
    expect(invoke).toHaveBeenLastCalledWith("update_marker", {
      markerId: 5,
      label: null,
      kind: "Chapter",
      completed: true,
    });

    await ipc.api.seek(1.5);
    expect(invoke).toHaveBeenLastCalledWith("preview_seek", { positionSecs: 1.5 });
  });

  it("resolve with what the command returns", async () => {
    invoke.mockResolvedValueOnce(42);
    await expect(ipc.api.startExport("out.mp4", false)).resolves.toBe(42);
  });
});

describe("event subscriptions", () => {
  it("listen on the named event and hand the payload to the handler", async () => {
    const subscriptions: [string, (handler: never) => Promise<unknown>][] = [
      [ipc.events.session, ipc.onSession],
      [ipc.events.preview, ipc.onPreview],
      [ipc.events.exports, ipc.onExports],
      [ipc.events.transcribe, ipc.onTranscribe],
      [ipc.events.sceneCut, ipc.onSceneCut],
      [ipc.events.motionTrack, ipc.onMotionTrack],
      [ipc.events.autoReframe, ipc.onAutoReframe],
      [ipc.events.matteGeneration, ipc.onMatteGeneration],
      [ipc.events.privacyBlur, ipc.onPrivacyBlur],
      [ipc.events.tts, ipc.onTts],
      [ipc.events.watchFolder, ipc.onWatchFolder],
      [ipc.events.updateCheck, ipc.onUpdateCheck],
      [ipc.events.youtubeDownload, ipc.onYoutubeDownload],
    ];
    for (const [eventName, subscribe] of subscriptions) {
      const handler = vi.fn();
      await expect(subscribe(handler as never)).resolves.toBe(unlisten);
      listeners.get(eventName)!({ payload: { sample: eventName } });
      expect(handler, eventName).toHaveBeenCalledWith({ sample: eventName });
    }
    expect(listen).toHaveBeenCalledTimes(subscriptions.length);
  });
});

describe("import failures and media commands", () => {
  it("hands only the file name of a failed import to the handler", async () => {
    const handler = vi.fn();
    await ipc.onImportFailed(handler);
    listeners.get(ipc.events.importFailed)!({ payload: { fileName: "broken.mp4" } });
    expect(handler).toHaveBeenCalledWith("broken.mp4");
  });

  it("calls the commands that remove media, reveal a path and lift a range", async () => {
    await ipc.api.removeAsset(4);
    expect(invoke).toHaveBeenLastCalledWith("remove_asset", { assetId: 4 });
    await ipc.api.revealInFolder("C:/out/a.mp4");
    expect(invoke).toHaveBeenLastCalledWith("reveal_in_folder", { path: "C:/out/a.mp4" });
    await ipc.api.liftRange(2, 6);
    expect(invoke).toHaveBeenLastCalledWith("lift_range", { startSecs: 2, endSecs: 6 });
  });
});

describe("export helper", () => {
  it("does nothing when the save dialog is dismissed", async () => {
    invoke.mockResolvedValueOnce(null);
    await ipc.exportActiveSequence(() => {
      return true;
    });
    expect(invoke).toHaveBeenCalledTimes(1);
    expect(invoke).toHaveBeenCalledWith("pick_export_path");
  });

  it("starts a plain export for a new file", async () => {
    invoke.mockResolvedValueOnce({ path: "a.mp4", exists: false });
    const confirm = vi.fn(() => {
      return true;
    });
    await ipc.exportActiveSequence(confirm);
    expect(confirm).not.toHaveBeenCalled();
    expect(invoke).toHaveBeenLastCalledWith("start_export", { path: "a.mp4", rename: false });
  });

  it("overwrites an existing file only after confirmation, otherwise renames", async () => {
    invoke.mockResolvedValueOnce({ path: "a.mp4", exists: true });
    await ipc.exportActiveSequence(() => {
      return true;
    });
    expect(invoke).toHaveBeenLastCalledWith("start_export", { path: "a.mp4", rename: false });

    invoke.mockResolvedValueOnce({ path: "a.mp4", exists: true });
    await ipc.exportActiveSequence(() => {
      return false;
    });
    expect(invoke).toHaveBeenLastCalledWith("start_export", { path: "a.mp4", rename: true });
  });
});

describe("media urls", () => {
  it("use the custom scheme on every platform but Windows", () => {
    expect(ipc.frameUrl(3)).toBe("oca://localhost/frame/3");
    expect(ipc.thumbnailUrl(9)).toBe("oca://localhost/thumb/9");
  });

  it("use the localhost host form on Windows", async () => {
    vi.resetModules();
    vi.stubGlobal("navigator", { userAgent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64)" });
    const windows = await import("../src/ipc");
    expect(windows.frameUrl(3)).toBe("http://oca.localhost/frame/3");
    expect(windows.thumbnailUrl(9)).toBe("http://oca.localhost/thumb/9");
    vi.unstubAllGlobals();
  });
});

describe("window close", () => {
  it("passes the reasons of a held-back close to the handler and quits on request", async () => {
    const handler = vi.fn();
    await ipc.onCloseRequested(handler);
    listeners.get(ipc.events.closeRequested)!({ payload: { dirty: true, exporting: false } });
    expect(handler).toHaveBeenCalledWith({ dirty: true, exporting: false });

    await ipc.api.quitApp();
    expect(invoke).toHaveBeenLastCalledWith("quit_app");
  });
});

describe("preview recovery", () => {
  it("hands the restart flag of a playback crash to the handler", async () => {
    const handler = vi.fn();
    await ipc.onPreviewRecovery(handler);
    listeners.get(ipc.events.previewRecovery)!({ payload: { restarted: false } });
    expect(handler).toHaveBeenCalledWith(false);
  });
});
