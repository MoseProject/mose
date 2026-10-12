import { cx } from "../cx";
import baseStyles from "../styles/base.module.css";
import styles from "./preview.module.css";
import { useEffect, useRef, useState } from "react";
import { t, type TextKey } from "../i18n";
import { scopeUrl } from "../ipc";
import { patchUi, useStore, uiStore } from "../store";
import { SCOPE_KINDS, type ScopeKind } from "../types";

const LABELS: Record<ScopeKind, TextKey> = {
  waveform: "scopeWaveform",
  parade: "scopeParade",
  histogram: "scopeHistogram",
  vectorscope: "scopeVectorscope",
};

/** At most this often a new scope is asked for while frames keep coming (playback, scrubbing). */
export const SCOPE_REFRESH_MS = 250;

/** The colour scope of the frame the monitor shows: waveform, RGB parade, histogram or
 *  vectorscope. Drawn by the backend on request, so it costs nothing while the strip is closed. */
export function ScopesPanel({ frameSeq }: { frameSeq: number }) {
  const ui = useStore(uiStore);
  const [shownSeq, setShownSeq] = useState(frameSeq);
  const lastAskedAt = useRef(0);
  const pending = useRef<number | null>(null);

  useEffect(() => {
    if (frameSeq === shownSeq) {
      return;
    }
    const wait = Math.max(0, lastAskedAt.current + SCOPE_REFRESH_MS - Date.now());
    if (pending.current !== null) {
      window.clearTimeout(pending.current);
    }
    pending.current = window.setTimeout(() => {
      pending.current = null;
      lastAskedAt.current = Date.now();
      setShownSeq(frameSeq);
    }, wait);
    return () => {
      if (pending.current !== null) {
        window.clearTimeout(pending.current);
        pending.current = null;
      }
    };
  }, [frameSeq, shownSeq]);

  return (
    <div
      className={cx(styles, baseStyles, "scopes")}
      data-testid="scopes-panel"
    >
      <select
        aria-label={t("scopes")}
        value={ui.scopeKind}
        onChange={(e) => {
          patchUi({ scopeKind: e.target.value as ScopeKind });
        }}
      >
        {SCOPE_KINDS.map((kind) => {
          return (
            <option
              key={kind}
              value={kind}
            >
              {t(LABELS[kind])}
            </option>
          );
        })}
      </select>
      <img
        className={cx(styles, baseStyles, `scope-image ${ui.scopeKind}`)}
        src={scopeUrl(ui.scopeKind, shownSeq)}
        alt={t(LABELS[ui.scopeKind])}
        draggable={false}
        data-testid="scope-image"
      />
    </div>
  );
}
