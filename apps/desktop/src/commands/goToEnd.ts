import { timelineDuration } from "../editing";
import { api } from "../ipc";
import { run, sessionStore } from "../store";
import type { Command } from "./types";
import { hasSession } from "./helpers";

export const goToEndCommand: Command = {
  id: "goToEnd",
  label: "goToEnd",
  group: "playback",
  keys: [{ key: "End" }],
  enabled: hasSession,
  run: () => {
    const session = sessionStore.get();
    return session ? run(api.seek(timelineDuration(session))) : undefined;
  },
};
