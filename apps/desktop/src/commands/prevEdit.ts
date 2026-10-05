import { editPoints, neighbourEditPoint } from "../editing";
import { api } from "../ipc";
import { previewStore, run, sessionStore } from "../store";
import type { Command } from "./types";
import { hasSession } from "./helpers";

export const prevEditCommand: Command = {
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
};
