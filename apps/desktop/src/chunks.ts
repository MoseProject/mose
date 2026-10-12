// The screens and dialogs that are fetched on first use, in one place so that they can also be
// fetched ahead of it: the Editor while the home screen is idle, the export dialog when the pointer
// reaches its button. A fetched module is cached, so the lazy component that follows renders at
// once.

export const loadEditor = () => {
  return import("./screens/Editor");
};
export const loadExportDialog = () => {
  return import("@components/export/ExportDialog");
};
export const loadExportPanel = () => {
  return import("@components/export/ExportPanel");
};
export const loadPrefs = () => {
  return import("./screens/Prefs");
};
export const loadShortcutsDialog = () => {
  return import("@components/chrome/ShortcutsDialog");
};
export const loadWatchFolder = () => {
  return import("./screens/WatchFolder");
};
export const loadCommandPalette = () => {
  return import("@components/chrome/CommandPalette");
};

/** Runs `task` when the browser has nothing better to do (a timer where it has no idle callback). */
export function whenIdle(task: () => void): () => void {
  if (typeof requestIdleCallback === "function") {
    const id = requestIdleCallback(task, { timeout: 4000 });
    return () => {
      cancelIdleCallback(id);
    };
  }
  const id = setTimeout(task, 1500);
  return () => {
    clearTimeout(id);
  };
}
