import toolrailStyles from "../toolrail.module.css";
import libraryStyles from "../library/library.module.css";
import topbarStyles from "../../topbar/topbar.module.css";
import { cx } from "../../cx";
import baseStyles from "../../styles/base.module.css";
import styles from "./chrome.module.css";
import { Icon } from "./Icon";
import { useEffect, useRef, useState, type ReactNode } from "react";

/** A toolbar dropdown. The popover stays mounted and is only hidden with `visibility`, so a
 *  modal that one of its entries owns (templates, transcript, silence review) survives the menu
 *  closing: `.modal-overlay` re-enables visibility for itself. */
export function Menu({
  label,
  ariaLabel,
  children,
  testId,
  variant,
}: {
  label: ReactNode;
  ariaLabel?: string;
  children: ReactNode;
  testId?: string;
  variant?: "bar";
}) {
  const [open, setOpen] = useState(false);
  const [pos, setPos] = useState({ left: 0, top: 0 });
  const root = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) {
      return;
    }
    const onPointer = (e: PointerEvent) => {
      if (!root.current?.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        setOpen(false);
      }
    };
    document.addEventListener("pointerdown", onPointer);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("pointerdown", onPointer);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  const toggle = (e: React.MouseEvent<HTMLButtonElement>) => {
    const rect = e.currentTarget.getBoundingClientRect();
    setPos({ left: rect.left, top: rect.bottom + 2 });
    setOpen((value) => {
      return !value;
    });
  };

  return (
    <div
      className={cx(styles, baseStyles, toolrailStyles, libraryStyles, topbarStyles, "menu")}
      ref={root}
    >
      <button
        className={cx(
          styles,
          baseStyles,
          toolrailStyles,
          libraryStyles,
          topbarStyles,
          variant === "bar" ? "menu-trigger menubar-item" : "icon menu-trigger",
        )}
        data-testid={testId}
        aria-label={ariaLabel}
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={toggle}
      >
        {label}
        {typeof label === "string" && variant !== "bar" && (
          <Icon
            name="chevronDown"
            size={12}
          />
        )}
      </button>
      <div
        className={cx(styles, baseStyles, `menu-pop ${open ? "open" : ""}`)}
        role="menu"
        style={pos}
        onClick={(e) => {
          if (
            !(e.target as HTMLElement).closest(".modal-overlay") &&
            (e.target as HTMLElement).closest("button")
          ) {
            setOpen(false);
          }
        }}
      >
        {children}
      </div>
    </div>
  );
}
