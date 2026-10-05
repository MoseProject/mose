import { type TextKey } from "../i18n";
import { type KeySpec } from "../shortcuts";

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
