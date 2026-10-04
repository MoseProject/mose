// The single table of editor commands: what each does, where it belongs, and its key binding.
// The global keyboard handler, the shortcut sheet, the command palette and the tooltips all read
// this list, so a binding is written once and cannot drift between them.

import {
  addShapeOverlay,
  addTextOverlay,
  allClipIds,
  editPoints,
  neighbourEditPoint,
  seekToMarker,
  timelineAction,
  timelineDuration,
} from "./editing";
import { t, type TextKey } from "./i18n";
import { api } from "./ipc";
import { effectiveKeys, keymapStore, type Bindable } from "./keymap";
import { formatKeys, matchesKeys, type KeySpec } from "./shortcuts";
import { playRange, shuttle, stopShuttle } from "./shuttle";
import { patchUi, previewStore, run, sessionStore, uiStore, type EditorTool } from "./store";

export type CommandGroup = "playback" | "edit" | "timeline" | "tools" | "file" | "view";

export interface Command {
  id: string;
  label: TextKey;
  group: CommandGroup;
  /** Bindings; the first one is the one shown to people. */
  keys?: KeySpec[];
  /** Whether holding the key keeps running the command. Everything else runs once per press, so
   *  a held key cannot toggle playback, split or add markers over and over. */
  repeatable?: boolean;
  /** False keeps the command out of the palette (it is still a shortcut). */
  palette?: boolean;
  enabled?: () => boolean;
  run: () => void | Promise<unknown>;
}

export const GROUP_LABELS: Record<CommandGroup, TextKey> = {
  playback: "groupPlayback",
  edit: "groupEdit",
  timeline: "groupTimeline",
  tools: "groupTools",
  file: "groupFile",
  view: "groupView",
};

const selected = () => {
  return uiStore.get().selectedClipIds;
};
const hasSession = () => {
  return sessionStore.get() !== null;
};
const hasSelection = () => {
  return selected().length > 0;
};
const seekBy = (secs: number) => {
  const target = Math.max(0, previewStore.get().playheadSecs + secs);
  return run(api.seek(target));
};
const setTool = (tool: EditorTool) => {
  return () => {
    patchUi({ tool });
  };
};
const activeMarkers = () => {
  const session = sessionStore.get();
  return session ? session.project.sequences[session.project.active_sequence].timeline.markers : [];
};

export const COMMANDS: Command[] = [
  // ---- playback
  {
    id: "playPause",
    label: "cmdPlayPause",
    group: "playback",
    keys: [{ code: "Space" }],
    run: () => {
      stopShuttle();
      return run(api.setPlaying(!previewStore.get().playing));
    },
  },
  {
    id: "play",
    label: "cmdPlay",
    group: "playback",
    keys: [{ key: "l" }],
    run: () => {
      return shuttle(1);
    },
  },
  {
    id: "shuttleBack",
    label: "cmdShuttleBack",
    group: "playback",
    keys: [{ key: "j" }],
    run: () => {
      return shuttle(-1);
    },
  },
  {
    id: "pause",
    label: "cmdPause",
    group: "playback",
    keys: [{ key: "k" }],
    run: () => {
      stopShuttle();
      return run(api.setPlaying(false));
    },
  },
  {
    id: "back1s",
    repeatable: true,
    label: "cmdBack1s",
    group: "playback",
    run: () => {
      return seekBy(-1);
    },
  },
  {
    id: "back10s",
    repeatable: true,
    label: "cmdBack10s",
    group: "playback",
    keys: [{ key: "j", shift: true }],
    palette: false,
    run: () => {
      return seekBy(-10);
    },
  },
  {
    id: "forward10s",
    repeatable: true,
    label: "cmdForward10s",
    group: "playback",
    keys: [{ key: "l", shift: true }],
    palette: false,
    run: () => {
      return seekBy(10);
    },
  },
  {
    id: "stepBack",
    repeatable: true,
    label: "stepBack",
    group: "playback",
    keys: [{ key: "ArrowLeft" }],
    run: () => {
      return run(api.step(-1));
    },
  },
  {
    id: "stepForward",
    repeatable: true,
    label: "stepForward",
    group: "playback",
    keys: [{ key: "ArrowRight" }],
    run: () => {
      return run(api.step(1));
    },
  },
  {
    id: "stepBack10",
    repeatable: true,
    label: "cmdStepBack10",
    group: "playback",
    keys: [{ key: "ArrowLeft", shift: true }],
    palette: false,
    run: () => {
      return run(api.step(-10));
    },
  },
  {
    id: "stepForward10",
    repeatable: true,
    label: "cmdStepForward10",
    group: "playback",
    keys: [{ key: "ArrowRight", shift: true }],
    palette: false,
    run: () => {
      return run(api.step(10));
    },
  },
  {
    id: "goToStart",
    label: "goToStart",
    group: "playback",
    keys: [{ key: "Home" }],
    run: () => {
      return run(api.seek(0));
    },
  },
  {
    id: "goToEnd",
    label: "goToEnd",
    group: "playback",
    keys: [{ key: "End" }],
    enabled: hasSession,
    run: () => {
      const session = sessionStore.get();
      return session ? run(api.seek(timelineDuration(session))) : undefined;
    },
  },
  {
    id: "prevEdit",
    repeatable: true,
    label: "cmdPrevEdit",
    group: "playback",
    keys: [{ key: "ArrowUp" }],
    enabled: hasSession,
    run: () => {
      const session = sessionStore.get();
      const target = session
        ? neighbourEditPoint(editPoints(session), previewStore.get().playheadSecs, -1)
        : null;
      return target === null ? undefined : run(api.seek(target));
    },
  },
  {
    id: "nextEdit",
    repeatable: true,
    label: "cmdNextEdit",
    group: "playback",
    keys: [{ key: "ArrowDown" }],
    enabled: hasSession,
    run: () => {
      const session = sessionStore.get();
      const target = session
        ? neighbourEditPoint(editPoints(session), previewStore.get().playheadSecs, 1)
        : null;
      return target === null ? undefined : run(api.seek(target));
    },
  },
  // ---- editing
  {
    id: "undo",
    repeatable: true,
    label: "undo",
    group: "edit",
    keys: [{ key: "z", ctrl: true }],
    run: () => {
      return run(api.undo());
    },
  },
  {
    id: "redo",
    repeatable: true,
    label: "redo",
    group: "edit",
    keys: [
      { key: "z", ctrl: true, shift: true },
      { key: "y", ctrl: true },
    ],
    run: () => {
      return run(api.redo());
    },
  },
  {
    id: "split",
    label: "cmdSplit",
    group: "edit",
    keys: [{ key: "s", anyShift: true }, { key: "c" }],
    run: () => {
      return run(api.splitAtPlayhead(selected()));
    },
  },
  {
    id: "delete",
    label: "delete",
    group: "edit",
    keys: [{ key: "Delete" }, { key: "Backspace" }],
    enabled: hasSelection,
    run: () => {
      return run(api.deleteClips(selected(), false));
    },
  },
  {
    id: "rippleDelete",
    label: "rippleDelete",
    group: "edit",
    keys: [
      { key: "Delete", shift: true },
      { key: "Backspace", shift: true },
    ],
    enabled: hasSelection,
    run: () => {
      return run(api.deleteClips(selected(), true));
    },
  },
  {
    id: "copy",
    label: "menuCopy",
    group: "edit",
    keys: [{ key: "c", ctrl: true }],
    enabled: hasSelection,
    run: () => {
      return run(api.copyClip(selected()[0]));
    },
  },
  {
    id: "cut",
    label: "menuCut",
    group: "edit",
    keys: [{ key: "x", ctrl: true }],
    enabled: hasSelection,
    run: () => {
      return run(api.cutClip(selected()[0]));
    },
  },
  {
    id: "paste",
    label: "menuPaste",
    group: "edit",
    keys: [{ key: "v", ctrl: true }],
    run: () => {
      return run(api.pasteClip()).then((ids) => {
        if (ids && ids.length > 0) {
          patchUi({ selectedClipIds: ids });
        }
      });
    },
  },
  {
    id: "duplicate",
    label: "cmdDuplicate",
    group: "edit",
    keys: [{ key: "d", ctrl: true }],
    enabled: hasSelection,
    run: async () => {
      await run(api.copyClip(selected()[0]));
      const ids = await run(api.pasteClip());
      if (ids && ids.length > 0) {
        patchUi({ selectedClipIds: ids });
      }
    },
  },
  {
    id: "copyFormat",
    label: "cmdCopyFormat",
    group: "edit",
    keys: [{ key: "c", ctrl: true, shift: true }],
    enabled: hasSelection,
    run: () => {
      return run(api.copyClipFormatting(selected()[0]));
    },
  },
  {
    id: "pasteFormat",
    label: "cmdPasteFormat",
    group: "edit",
    keys: [{ key: "v", ctrl: true, shift: true }],
    enabled: hasSelection,
    run: () => {
      return run(api.pasteClipFormatting(selected()[0]));
    },
  },
  {
    id: "selectAll",
    label: "cmdSelectAll",
    group: "edit",
    keys: [{ key: "a", ctrl: true }],
    enabled: hasSession,
    run: () => {
      const session = sessionStore.get();
      if (session) {
        patchUi({ selectedClipIds: allClipIds(session) });
      }
    },
  },
  {
    id: "deselect",
    label: "cmdDeselect",
    group: "edit",
    keys: [{ key: "Escape" }],
    run: () => {
      patchUi({ selectedClipIds: [] });
    },
  },
  {
    id: "addTextClip",
    label: "addTextClip",
    group: "edit",
    enabled: hasSession,
    run: () => {
      return addTextOverlay();
    },
  },
  {
    id: "addShapeClip",
    label: "addShapeClip",
    group: "edit",
    enabled: hasSession,
    run: () => {
      return addShapeOverlay();
    },
  },
  // ---- timeline
  {
    id: "addMarker",
    label: "addMarker",
    group: "timeline",
    keys: [{ key: "m" }],
    run: () => {
      return run(api.addMarker());
    },
  },
  {
    id: "prevMarker",
    repeatable: true,
    label: "previousMarker",
    group: "timeline",
    keys: [{ key: "m", shift: true }],
    enabled: hasSession,
    run: () => {
      seekToMarker(activeMarkers(), -1);
    },
  },
  {
    id: "nextMarker",
    repeatable: true,
    label: "nextMarker",
    group: "timeline",
    keys: [{ key: "m", alt: true }],
    enabled: hasSession,
    run: () => {
      seekToMarker(activeMarkers(), 1);
    },
  },
  {
    id: "markIn",
    label: "cmdMarkIn",
    group: "timeline",
    keys: [{ key: "i" }],
    run: () => {
      patchUi({ markIn: previewStore.get().playheadSecs });
    },
  },
  {
    id: "markOut",
    label: "cmdMarkOut",
    group: "timeline",
    keys: [{ key: "o" }],
    run: () => {
      patchUi({ markOut: previewStore.get().playheadSecs });
    },
  },
  {
    id: "clearMarks",
    label: "cmdClearMarks",
    group: "timeline",
    keys: [{ key: "x", alt: true }],
    run: () => {
      patchUi({ markIn: null, markOut: null });
    },
  },
  {
    id: "deleteRange",
    label: "cmdDeleteRange",
    group: "timeline",
    keys: [{ key: "Delete", ctrl: true, shift: true }],
    enabled: () => {
      const { markIn, markOut } = uiStore.get();
      return markIn !== null && markOut !== null && markOut > markIn;
    },
    run: async () => {
      const { markIn, markOut } = uiStore.get();
      if (markIn === null || markOut === null || markOut <= markIn) {
        return;
      }
      await run(api.rippleDeleteRange(markIn, markOut));
      patchUi({ markIn: null, markOut: null });
    },
  },
  {
    id: "liftRange",
    label: "cmdLiftRange",
    group: "timeline",
    keys: [{ key: ";" }],
    enabled: () => {
      const { markIn, markOut } = uiStore.get();
      return markIn !== null && markOut !== null && markOut > markIn;
    },
    run: async () => {
      const { markIn, markOut } = uiStore.get();
      if (markIn === null || markOut === null || markOut <= markIn) {
        return;
      }
      await run(api.liftRange(markIn, markOut));
      patchUi({ markIn: null, markOut: null });
    },
  },
  {
    id: "goToMarkIn",
    label: "cmdGoToMarkIn",
    group: "timeline",
    keys: [{ key: "i", shift: true }],
    enabled: () => {
      return uiStore.get().markIn !== null;
    },
    run: () => {
      const { markIn } = uiStore.get();
      return markIn === null ? undefined : run(api.seek(markIn));
    },
  },
  {
    id: "goToMarkOut",
    label: "cmdGoToMarkOut",
    group: "timeline",
    keys: [{ key: "o", shift: true }],
    enabled: () => {
      return uiStore.get().markOut !== null;
    },
    run: () => {
      const { markOut } = uiStore.get();
      return markOut === null ? undefined : run(api.seek(markOut));
    },
  },
  {
    id: "playRange",
    label: "cmdPlayRange",
    group: "playback",
    keys: [{ key: "p", shift: true }],
    enabled: () => {
      const { markIn, markOut } = uiStore.get();
      return markIn !== null && markOut !== null && markOut > markIn;
    },
    run: () => {
      return playRange();
    },
  },
  {
    id: "zoomIn",
    repeatable: true,
    label: "zoomIn",
    group: "timeline",
    keys: [{ key: "=" }, { key: "+", shift: true }, { key: "=", ctrl: true }],
    run: () => {
      timelineAction("zoomIn");
    },
  },
  {
    id: "zoomOut",
    repeatable: true,
    label: "zoomOut",
    group: "timeline",
    keys: [{ key: "-" }, { key: "-", ctrl: true }],
    run: () => {
      timelineAction("zoomOut");
    },
  },
  {
    id: "zoomFit",
    label: "zoomFit",
    group: "timeline",
    keys: [{ key: "z", shift: true }, { key: "\\" }],
    run: () => {
      timelineAction("zoomFit");
    },
  },
  {
    id: "scrollToPlayhead",
    label: "cmdScrollToPlayhead",
    group: "timeline",
    run: () => {
      timelineAction("scrollToPlayhead");
    },
  },
  {
    id: "renameSequence",
    label: "cmdRenameSequence",
    group: "timeline",
    keys: [{ key: "F2" }],
    enabled: hasSession,
    run: () => {
      timelineAction("renameSequence");
    },
  },
  {
    id: "toggleSnap",
    label: "cmdToggleSnap",
    group: "timeline",
    keys: [{ key: "n" }],
    run: () => {
      patchUi({ snapEnabled: !uiStore.get().snapEnabled });
    },
  },
  // ---- tools
  {
    id: "toolSelect",
    label: "cmdToolSelect",
    group: "tools",
    keys: [{ key: "v" }],
    run: setTool("select"),
  },
  {
    id: "toolRoll",
    label: "cmdToolRoll",
    group: "tools",
    keys: [{ key: "r" }],
    run: setTool("roll"),
  },
  {
    id: "toolSlip",
    label: "cmdToolSlip",
    group: "tools",
    keys: [{ key: "y" }],
    run: setTool("slip"),
  },
  {
    id: "toolSlide",
    label: "cmdToolSlide",
    group: "tools",
    keys: [{ key: "u" }],
    run: setTool("slide"),
  },
  {
    id: "toolText",
    label: "cmdToolText",
    group: "tools",
    keys: [{ key: "t" }],
    run: setTool("text"),
  },
  // ---- file
  {
    id: "save",
    label: "save",
    group: "file",
    keys: [{ key: "s", ctrl: true }],
    run: () => {
      return run(api.saveProject(false));
    },
  },
  {
    id: "saveAs",
    label: "saveAs",
    group: "file",
    keys: [{ key: "s", ctrl: true, shift: true }],
    run: () => {
      return run(api.saveProject(true));
    },
  },
  {
    id: "import",
    label: "importFiles",
    group: "file",
    keys: [{ key: "i", ctrl: true }],
    run: () => {
      return run(api.importMediaDialog());
    },
  },
  {
    id: "export",
    label: "export",
    group: "file",
    keys: [{ key: "e", ctrl: true }],
    run: () => {
      patchUi({ exportDialogOpen: true });
    },
  },
  // ---- view
  {
    id: "shortcuts",
    label: "cmdShortcuts",
    group: "view",
    keys: [
      { key: "?", anyShift: true },
      { key: "/", ctrl: true },
    ],
    run: () => {
      patchUi({ shortcutsOpen: true });
    },
  },
  {
    id: "palette",
    label: "cmdPalette",
    group: "view",
    keys: [{ key: "k", ctrl: true }],
    palette: false,
    run: () => {
      patchUi({ paletteOpen: true });
    },
  },
  {
    id: "settings",
    label: "cmdSettings",
    group: "view",
    keys: [{ key: ",", ctrl: true }],
    run: () => {
      patchUi({ settingsOpen: true });
    },
  },
  {
    id: "toggleGuides",
    label: "cmdToggleGuides",
    group: "view",
    run: () => {
      patchUi({ monitorGuides: !uiStore.get().monitorGuides });
    },
  },
];

const BY_ID = new Map(
  COMMANDS.map((command) => {
    return [command.id, command];
  }),
);

export function findCommand(id: string): Command | undefined {
  return BY_ID.get(id);
}

/** The bindings a command answers to now: the user's changes over its defaults. */
export function keysOf(command: Command): KeySpec[] {
  return effectiveKeys(command.id, command.keys, keymapStore.get());
}

/** Every command as a plain `{ id, keys }` row, for the shortcut editor's conflict checks. */
export function bindables(): Bindable[] {
  return COMMANDS.map((command) => {
    return { id: command.id, keys: command.keys };
  });
}

/** The binding shown for a command (`Ctrl+Z`), or `undefined` when it has none. */
export function shortcutLabel(id: string): string | undefined {
  const command = BY_ID.get(id);
  const keys = command ? keysOf(command) : [];
  return keys.length > 0 ? formatKeys(keys[0]) : undefined;
}

/** `Label (Ctrl+Z)` for a tooltip. */
export function withShortcut(label: string, id: string): string {
  const keys = shortcutLabel(id);
  return keys ? label + " (" + keys + ")" : label;
}

/** The command a key press is bound to, if any. */
export function commandForEvent(e: KeyboardEvent): Command | undefined {
  return COMMANDS.find((command) => {
    return keysOf(command).some((spec) => {
      return matchesKeys(e, spec);
    });
  });
}

/** Human label of a command in the current language. */
export function commandLabel(command: Command): string {
  return t(command.label);
}
