// Small editing actions shared by the timeline toolbar and the top-bar menus.

import { t } from "./i18n";
import { api } from "./ipc";
import { patchUi, previewStore, run } from "./store";
import type { Marker } from "./types";

/** Moves the playhead to the nearest marker before (`-1`) or after (`1`) it, if any. */
export function seekToMarker(markers: Marker[], direction: -1 | 1) {
  const playhead = previewStore.get().playheadSecs;
  const candidates =
    direction < 0
      ? markers
          .filter((m) => {
            return m.position_secs < playhead;
          })
          .sort((a, b) => {
            return b.position_secs - a.position_secs;
          })
      : markers
          .filter((m) => {
            return m.position_secs > playhead;
          })
          .sort((a, b) => {
            return a.position_secs - b.position_secs;
          });
  if (candidates[0]) {
    void run(api.seek(candidates[0].position_secs));
  }
}

/** Creates a text/shape clip at the playhead and selects it, as egui's "+ Add" buttons do. */
export async function addOverlay(call: Promise<number | null>) {
  const id = await run(call);
  if (id != null) {
    patchUi({ selectedClipIds: [id] });
  }
}

export const addTextOverlay = () => {
  return addOverlay(api.addTextClip(t("defaultTextContent")));
};
export const addShapeOverlay = () => {
  return addOverlay(api.addShapeClip());
};
