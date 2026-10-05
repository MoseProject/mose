import { api } from "../ipc";
import { patchUi, previewStore, run, sessionStore, uiStore, type EditorTool } from "../store";

export const selected = () => {
  return uiStore.get().selectedClipIds;
};
export const hasSession = () => {
  return sessionStore.get() !== null;
};
export const hasSelection = () => {
  return selected().length > 0;
};
export const seekBy = (secs: number) => {
  const target = Math.max(0, previewStore.get().playheadSecs + secs);
  return run(api.seek(target));
};
export const setTool = (tool: EditorTool) => {
  return () => {
    patchUi({ tool });
  };
};
export const activeMarkers = () => {
  const session = sessionStore.get();
  return session ? session.project.sequences[session.project.active_sequence].timeline.markers : [];
};
