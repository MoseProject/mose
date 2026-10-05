import { type ReactNode } from "react";
import { Icon } from "../Icon";
import { useDraft } from "../../useDraft";
import { t } from "../../i18n";

/** Collapsible property group, closed by default unless `defaultOpen` -- mirrors
 *  `crates/ui/src/components/property.rs`'s `property_section`. Uncontrolled (`<details
 *  open>` sets only the *initial* state): the parent `ClipControls` remounts on every clip
 *  selection change (`key={clip.id}` above), so each newly selected clip's sections
 *  recompute their own default-open state fresh, same per-clip behavior egui's
 *  `default_open` argument has. */
export type InspectorGroup = "inspector" | "effects" | "audio";

/** State of a section's keyframe diamond. */
export interface SectionKeyframe {
  /** A keyframe already sits at the playhead, so clicking removes it. */
  active: boolean;
  /** The playhead is outside the clip. */
  disabled: boolean;
  onToggle: () => void;
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
  return (
    <details
      className="property-section"
      open={defaultOpen}
      data-testid={testId}
      data-group={group}
    >
      <summary>
        <span>{label}</span>
        {keyframe && (
          <button
            className={"icon section-keyframe" + (keyframe.active ? " active" : "")}
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
        )}
        {onReset && (
          <button
            className="icon section-reset"
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
      <div className="property-section-body">{children}</div>
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
}) {
  const [draft, setDraft] = useDraft(props.value);
  const commit = () => {
    if (draft !== props.value) {
      props.onCommit(draft);
    }
  };
  return (
    <div className="field slider-field">
      <label>{props.label}</label>
      <input
        type="range"
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
      <span className="mono">{props.format(draft)}</span>
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
}: {
  label: string;
  value: number;
  step: number;
  disabled?: boolean;
  suffix?: string;
  onCommit: (value: number) => void;
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
    <label className="number-field">
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
      {suffix && <span className="muted">{suffix}</span>}
    </label>
  );
}
