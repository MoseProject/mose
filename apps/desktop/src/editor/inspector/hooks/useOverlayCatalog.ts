import { useEffect, useState } from "react";
import { api } from "../../../ipc";
import { run } from "../../../store";
import type { OverlayCatalog } from "../../../types";

let cachedCatalog: OverlayCatalog | null = null;
let pendingCatalog: Promise<OverlayCatalog | undefined> | null = null;

/** Core's static font/shape catalog, fetched once per session. `null` while loading. */
export function useOverlayCatalog(): OverlayCatalog | null {
  const [catalog, setCatalog] = useState(cachedCatalog);
  useEffect(() => {
    if (catalog) {
      return;
    }
    let alive = true;
    pendingCatalog ??= run(api.overlayCatalog());
    void pendingCatalog.then((loaded) => {
      if (!loaded) {
        pendingCatalog = null;
        return;
      }
      cachedCatalog = loaded;
      if (alive) {
        setCatalog(loaded);
      }
    });
    return () => {
      alive = false;
    };
  }, [catalog]);
  return catalog;
}
