// Inspector field widgets and catalog access shared by the text and shape inspectors. Every
// widget previews locally and commits once per user-visible change (one undo step).

import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { useDraft } from "../useDraft";
import { api } from "../ipc";
import { hexToRgb, rgbToHex } from "../overlays";
import { run } from "../store";
import type { OverlayCatalog } from "../types";

const FULL_ROW = { gridColumn: "1 / 3" } as const;

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
        pendingCatalog = null; // failed: let the next mount retry
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

/** Single-line text committed on blur or Enter, never per keystroke. */
export function TextField(props: {
  label: string;
  value: string;
  placeholder?: string;
  onCommit: (value: string) => void;
}) {
  const [draft, setDraft] = useDraft(props.value);
  const commit = () => {
    if (draft !== props.value) {
      props.onCommit(draft);
    }
  };
  return (
    <div className="field">
      <label>{props.label}</label>
      <input
        style={FULL_ROW}
        value={draft}
        placeholder={props.placeholder}
        onChange={(e) => {
          return setDraft(e.target.value);
        }}
        onBlur={commit}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            e.currentTarget.blur();
          }
          if (e.key === "Escape") {
            setDraft(props.value);
          }
        }}
      />
    </div>
  );
}

export function SelectField(props: {
  label: string;
  value: string;
  disabled?: boolean;
  onChange: (value: string) => void;
  children: ReactNode;
}) {
  return (
    <div className="field">
      <label>{props.label}</label>
      <select
        style={FULL_ROW}
        value={props.value}
        disabled={props.disabled}
        onChange={(e) => {
          return props.onChange(e.target.value);
        }}
      >
        {props.children}
      </select>
    </div>
  );
}

/** Color swatch that previews while the picker is open and commits once when it closes
 *  (the native `change` event; React's `onChange` fires on every `input`). */
export function ColorField(props: {
  label: string;
  rgb: readonly number[];
  onCommit: (rgb: [number, number, number]) => void;
}) {
  const value = rgbToHex(props.rgb);
  const [draft, setDraft] = useDraft(value);
  const inputRef = useRef<HTMLInputElement>(null);
  const commitRef = useRef(props.onCommit);
  const committedRef = useRef(value);
  useLayoutEffect(() => {
    commitRef.current = props.onCommit;
    committedRef.current = value;
  });
  useEffect(() => {
    const input = inputRef.current;
    if (!input) {
      return;
    }
    const onChange = () => {
      const rgb = hexToRgb(input.value);
      if (rgb && input.value.toLowerCase() !== committedRef.current) {
        commitRef.current(rgb);
      }
    };
    input.addEventListener("change", onChange);
    return () => {
      return input.removeEventListener("change", onChange);
    };
  }, []);
  return (
    <div className="field">
      <label>{props.label}</label>
      <input
        ref={inputRef}
        type="color"
        value={draft}
        onChange={(e) => {
          return setDraft(e.target.value);
        }}
      />
    </div>
  );
}
