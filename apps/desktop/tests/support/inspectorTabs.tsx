import type { ReactElement } from "react";
import { InspectorTabContext } from "../../src/editor/inspector/controls";

/** Builds every inspector section body, open or not and on show or not, so a test can reach a
 *  control without clicking through tabs and sections first. */
export function withAllSections(ui: ReactElement): ReactElement {
  return <InspectorTabContext.Provider value="all">{ui}</InspectorTabContext.Provider>;
}
