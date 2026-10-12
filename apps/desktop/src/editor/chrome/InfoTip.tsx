import { useId } from "react";
import { cx } from "../../cx";
import baseStyles from "../../styles/base.module.css";
import styles from "./chrome.module.css";
import { Icon } from "./Icon";

/** An info icon that shows `text` in a popover on hover and keyboard focus, for a note that
 *  belongs next to one control without taking a line of the layout. */
export function InfoTip({ label, text }: { label: string; text: string }) {
  const id = useId();
  return (
    <span className={cx(styles, baseStyles, "info-tip")}>
      <button
        type="button"
        className={cx(styles, baseStyles, "info-tip-button")}
        aria-label={label}
        aria-describedby={id}
      >
        <Icon
          name="info"
          size={14}
        />
      </button>
      <span
        id={id}
        role="tooltip"
        className={cx(styles, baseStyles, "info-tip-pop")}
      >
        {text}
      </span>
    </span>
  );
}
