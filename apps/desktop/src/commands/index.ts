// The single table of editor commands: what each does, where it belongs, and its key binding.
// The global keyboard handler, the shortcut sheet, the command palette and the tooltips all read
// this list, so a binding is written once and cannot drift between them. One file per command
// in this folder; the order below is the order of the palette and the shortcut sheet.

import { playPauseCommand } from "./playPause";
import { playCommand } from "./play";
import { shuttleBackCommand } from "./shuttleBack";
import { pauseCommand } from "./pause";
import { back1sCommand } from "./back1s";
import { back10sCommand } from "./back10s";
import { forward10sCommand } from "./forward10s";
import { stepBackCommand } from "./stepBack";
import { stepForwardCommand } from "./stepForward";
import { stepBack10Command } from "./stepBack10";
import { stepForward10Command } from "./stepForward10";
import { goToStartCommand } from "./goToStart";
import { goToEndCommand } from "./goToEnd";
import { prevEditCommand } from "./prevEdit";
import { nextEditCommand } from "./nextEdit";
import { undoCommand } from "./undo";
import { redoCommand } from "./redo";
import { splitCommand } from "./split";
import { deleteCommand } from "./delete";
import { rippleDeleteCommand } from "./rippleDelete";
import { copyCommand } from "./copy";
import { cutCommand } from "./cut";
import { pasteCommand } from "./paste";
import { duplicateCommand } from "./duplicate";
import { copyFormatCommand } from "./copyFormat";
import { pasteFormatCommand } from "./pasteFormat";
import { selectAllCommand } from "./selectAll";
import { deselectCommand } from "./deselect";
import { addTextClipCommand } from "./addTextClip";
import { addShapeClipCommand } from "./addShapeClip";
import { addMarkerCommand } from "./addMarker";
import { prevMarkerCommand } from "./prevMarker";
import { nextMarkerCommand } from "./nextMarker";
import { markInCommand } from "./markIn";
import { markOutCommand } from "./markOut";
import { clearMarksCommand } from "./clearMarks";
import { deleteRangeCommand } from "./deleteRange";
import { liftRangeCommand } from "./liftRange";
import { goToMarkInCommand } from "./goToMarkIn";
import { goToMarkOutCommand } from "./goToMarkOut";
import { playRangeCommand } from "./playRange";
import { zoomInCommand } from "./zoomIn";
import { zoomOutCommand } from "./zoomOut";
import { zoomFitCommand } from "./zoomFit";
import { scrollToPlayheadCommand } from "./scrollToPlayhead";
import { renameSequenceCommand } from "./renameSequence";
import { toggleSnapCommand } from "./toggleSnap";
import { toolSelectCommand } from "./toolSelect";
import { toolRollCommand } from "./toolRoll";
import { toolSlipCommand } from "./toolSlip";
import { toolSlideCommand } from "./toolSlide";
import { toolTextCommand } from "./toolText";
import { saveCommand } from "./save";
import { saveAsCommand } from "./saveAs";
import { importCommand } from "./import";
import { exportCommand } from "./export";
import { shortcutsCommand } from "./shortcuts";
import { paletteCommand } from "./palette";
import { settingsCommand } from "./settings";
import { toggleGuidesCommand } from "./toggleGuides";
import { t } from "../i18n";
import { effectiveKeys, keymapStore, type Bindable } from "../keymap";
import { formatKeys, matchesKeys, type KeySpec } from "../shortcuts";
import type { Command } from "./types";

export { GROUP_LABELS, type Command, type CommandGroup } from "./types";

export const COMMANDS: Command[] = [
  // ---- playback
  playPauseCommand,
  playCommand,
  shuttleBackCommand,
  pauseCommand,
  back1sCommand,
  back10sCommand,
  forward10sCommand,
  stepBackCommand,
  stepForwardCommand,
  stepBack10Command,
  stepForward10Command,
  goToStartCommand,
  goToEndCommand,
  prevEditCommand,
  nextEditCommand,
  // ---- editing
  undoCommand,
  redoCommand,
  splitCommand,
  deleteCommand,
  rippleDeleteCommand,
  copyCommand,
  cutCommand,
  pasteCommand,
  duplicateCommand,
  copyFormatCommand,
  pasteFormatCommand,
  selectAllCommand,
  deselectCommand,
  addTextClipCommand,
  addShapeClipCommand,
  // ---- timeline
  addMarkerCommand,
  prevMarkerCommand,
  nextMarkerCommand,
  markInCommand,
  markOutCommand,
  clearMarksCommand,
  deleteRangeCommand,
  liftRangeCommand,
  goToMarkInCommand,
  goToMarkOutCommand,
  playRangeCommand,
  zoomInCommand,
  zoomOutCommand,
  zoomFitCommand,
  scrollToPlayheadCommand,
  renameSequenceCommand,
  toggleSnapCommand,
  // ---- tools
  toolSelectCommand,
  toolRollCommand,
  toolSlipCommand,
  toolSlideCommand,
  toolTextCommand,
  // ---- file
  saveCommand,
  saveAsCommand,
  importCommand,
  exportCommand,
  // ---- view
  shortcutsCommand,
  paletteCommand,
  settingsCommand,
  toggleGuidesCommand,
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
