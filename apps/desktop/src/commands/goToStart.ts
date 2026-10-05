import { api } from "../ipc";
import { run } from "../store";
import type { Command } from "./types";

export const goToStartCommand: Command = {
  id: "goToStart",
  label: "goToStart",
  group: "playback",
  keys: [{ key: "Home" }],
  run: () => {
    return run(api.seek(0));
  },
};
