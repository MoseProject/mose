import { cx } from "../../cx";
import baseStyles from "../../styles/base.module.css";
import styles from "./inspector.module.css";
import { createContext, type ReactNode, useContext, useRef, useState } from "react";
import { Icon } from "../chrome/Icon";
import { useDraft } from "../../hooks/useDraft";
import { t } from "../../i18n";
import { rememberSection, sectionOverride } from "./memory";

/** Collapsible property group, closed by default unless `defaultOpen` -- mirrors
 *  `crates/ui/src/components/property.rs`'s `property_section`. Uncontrolled (`<details
 *  open>` sets only the *initial* state): the parent `ClipControls` remounts on every clip
 *  selection change (`key={clip.id}` above), so each newly selected clip's sections
 *  recompute their own default-open state fresh, same per-clip behavior egui's
 *  `default_open` argument has. */
export type InspectorGroup = "inspector" | "effects" | "audio";

/** The inspector tab on show. A section of another tab keeps only its header: its body is not
 *  built until its tab is on show. `"all"` builds every body whether or not its section is open
 *  or on show, for tests that drive controls without clicking through tabs and sections. */
export const InspectorTabContext = createContext<InspectorGroup | "all" | null>(null);

/** State of a section's keyframe diamond. */
export interface SectionKeyframe {
  /** A keyframe already sits at the playhead, so clicking removes it. */
  active: boolean;
  /** The playhead is outside the clip. */
  disabled: boolean;
  onToggle: () => void;
}

/** The diamond that adds or removes a keyframe at the playhead, for a section header or a row. */
export function KeyframeDiamond({
  keyframe,
  label,
  className = "row-keyframe",
}: {
  keyframe: SectionKeyframe;
  label: string;
  className?: string;
}) {
  return (
    <button
      className={cx(styles, baseStyles, "icon " + className + (keyframe.active ? " active" : ""))}
      aria-label={t("keyframeToggle") + ": " + label}
      aria-pressed={keyframe.active}
      title={keyframe.disabled ? t("keyframeOutside") : t("keyframeToggle")}
      disabled={keyframe.disabled}
      onClick={(e) => {
        e.preventDefault();
        e.stopPropagation();
        keyframe.onToggle();
      }}
    >
      <Icon name="keyframe" />
    </button>
  );
}

export function Section({
  label,
  defaultOpen,
  testId,
  group = "inspector",
  onReset,
  keyframe,
  children,
}: {
  label: string;
  defaultOpen: boolean;
  testId?: string;
  group?: InspectorGroup;
  /** Shows a reset button in the header that puts this section back to its defaults. */
  onReset?: () => void;
  /** Shows a diamond in the header that adds or removes a keyframe at the playhead. */
  keyframe?: SectionKeyframe;
  children: ReactNode;
}) {
  // What the user last chose for this section wins over the clip-derived default.
  const memoryKey = testId ?? label;
  const [initial] = useState(() => {
    return sectionOverride(memoryKey) ?? defaultOpen;
  });
  const lastOpen = useRef(initial);
  // The body is built the first time the section is open (and its tab is on show), then kept:
  // a closed section, or one on another tab, costs its header and nothing else.
  const [everOpened, setEverOpened] = useState(initial);
  const tab = useContext(InspectorTabContext);
  const bodyBuilt = tab === "all" || (everOpened && (tab === null || tab === group));
  return (
    <details
      className={cx(styles, baseStyles, "property-section")}
      open={initial}
      data-testid={testId}
      data-group={group}
      onToggle={(e) => {
        const open = e.currentTarget.open;
        // The event also fires once on mount, with the state it was rendered in.
        if (open !== lastOpen.current) {
          lastOpen.current = open;
          rememberSection(memoryKey, open);
        }
        if (open) {
          setEverOpened(true);
        }
      }}
    >
      <summary>
        <span>{label}</span>
        {keyframe && (
          <KeyframeDiamond
            keyframe={keyframe}
            label={label}
            className="section-keyframe"
          />
        )}
        {onReset && (
          <button
            className={cx(styles, baseStyles, "icon section-reset")}
            aria-label={t("resetSection") + ": " + label}
            title={t("resetSection")}
            onClick={(e) => {
              e.preventDefault();
              e.stopPropagation();
              onReset();
            }}
          >
            <Icon name="reset" />
          </button>
        )}
      </summary>
      <div className={cx(styles, baseStyles, "property-section-body")}>
        {bodyBuilt ? children : null}
      </div>
    </details>
  );
}

/** A range input that previews locally while dragging and commits once on release. */
export function Slider(props: {
  label: string;
  value: number;
  min: number;
  max: number;
  step: number;
  format: (value: number) => string;
  onCommit: (value: number) => void;
  /** Adds a diamond that keyframes this property at the playhead. */
  keyframe?: SectionKeyframe;
}) {
  const [draft, setDraft] = useDraft(props.value);
  const commit = () => {
    if (draft !== props.value) {
      props.onCommit(draft);
    }
  };
  return (
    <div
      className={cx(
        styles,
        baseStyles,
        "field slider-field" + (props.keyframe ? " keyframed" : ""),
      )}
    >
      <label>{props.label}</label>
      <input
        type="range"
        aria-label={props.label}
        min={props.min}
        max={props.max}
        step={props.step}
        value={draft}
        onChange={(e) => {
          return setDraft(Number(e.target.value));
        }}
        onPointerUp={commit}
        onKeyUp={commit}
        onBlur={commit}
      />
      <span className={cx(styles, baseStyles, "mono")}>{props.format(draft)}</span>
      {props.keyframe && (
        <KeyframeDiamond
          keyframe={props.keyframe}
          label={props.label}
        />
      )}
    </div>
  );
}

/** One numeric inspector field, committed on blur or Enter rather than per keystroke. */
export function NumberField({
  label,
  value,
  step,
  disabled,
  suffix,
  onCommit,
  keyframe,
}: {
  label: string;
  value: number;
  step: number;
  disabled?: boolean;
  suffix?: string;
  onCommit: (value: number) => void;
  /** Adds a diamond that keyframes this property at the playhead. */
  keyframe?: SectionKeyframe;
}) {
  const shown = String(Number(value.toFixed(2)));
  const [draft, setDraft] = useDraft(shown);
  const commit = () => {
    const parsed = Number(draft);
    if (draft.trim() !== "" && Number.isFinite(parsed) && String(parsed) !== shown) {
      onCommit(parsed);
    } else {
      setDraft(shown);
    }
  };
  return (
    <label className={cx(styles, baseStyles, "number-field")}>
      <span>{label}</span>
      <input
        type="number"
        aria-label={label}
        step={step}
        value={draft}
        disabled={disabled}
        onChange={(e) => {
          setDraft(e.target.value);
        }}
        onBlur={commit}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            e.currentTarget.blur();
          }
        }}
      />
      {suffix && <span className={cx(styles, baseStyles, "muted")}>{suffix}</span>}
      {keyframe && (
        <KeyframeDiamond
          keyframe={keyframe}
          label={label}
        />
      )}
    </label>
  );
}
