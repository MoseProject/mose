/** What the inspector remembers between clips and sessions: the last tab and which sections the
 *  user opened or closed. Per-viewer convenience only, so every storage access is guarded and
 *  the panel behaves the same without it. */
import type { InspectorGroup } from "./controls";

const TAB_KEY = "oca.inspector.tab";
const SECTIONS_KEY = "oca.inspector.sections";
const TABS: readonly InspectorGroup[] = ["inspector", "effects", "audio"];

export function loadTab(): InspectorGroup {
  try {
    const stored = localStorage.getItem(TAB_KEY);
    return (
      TABS.find((tab) => {
        return tab === stored;
      }) ?? "inspector"
    );
  } catch {
    return "inspector";
  }
}

export function saveTab(tab: InspectorGroup): void {
  try {
    localStorage.setItem(TAB_KEY, tab);
  } catch {
    // storage unavailable: the choice just is not kept
  }
}

function loadSections(): Record<string, boolean> {
  try {
    const parsed: unknown = JSON.parse(localStorage.getItem(SECTIONS_KEY) ?? "{}");
    return parsed !== null && typeof parsed === "object" ? (parsed as Record<string, boolean>) : {};
  } catch {
    return {};
  }
}

/** Whether the user left section `id` open (true) or closed (false); undefined if never touched. */
export function sectionOverride(id: string | undefined): boolean | undefined {
  if (!id) {
    return undefined;
  }
  const value = loadSections()[id];
  return typeof value === "boolean" ? value : undefined;
}

export function rememberSection(id: string | undefined, open: boolean): void {
  if (!id) {
    return;
  }
  try {
    localStorage.setItem(SECTIONS_KEY, JSON.stringify({ ...loadSections(), [id]: open }));
  } catch {
    // storage unavailable: the choice just is not kept
  }
}
