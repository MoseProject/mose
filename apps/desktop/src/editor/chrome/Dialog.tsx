import homeStyles from "../../screens/home.module.css";
import { cx } from "../../cx";
import baseStyles from "../../styles/base.module.css";
import styles from "./chrome.module.css";
import { useEffect, useId, useRef, type ReactNode } from "react";
import { uiStore } from "../../store";

const FOCUSABLE =
  'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

/** How a dialog leaves: a plain fade, or a flight to a point (offsets in pixels from where the
 *  panel is, `scale` the size it shrinks to). */
export type DialogExit = "fade" | { dx: number; dy: number; scale: number };

/** A modal dialog: labelled, Escape and backdrop close it, Tab stays inside, focus returns to the
 *  control that opened it, and global shortcuts stand down while it is open. */
export function Dialog({
  title,
  onClose,
  dismissible = true,
  wide = false,
  leaving = null,
  children,
  actions,
}: {
  title: string;
  onClose: () => void;
  /** False for a decision the user must make: Escape and the backdrop do nothing. */
  dismissible?: boolean;
  /** A wider panel for forms and lists. */
  wide?: boolean;
  /** Plays the exit: `"fade"` dims the dialog away; an offset and scale make the panel shrink
   *  and travel by that many pixels, towards a control. The caller removes the dialog once the
   *  motion is over. */
  leaving?: DialogExit | null;
  children: ReactNode;
  actions?: ReactNode;
}) {
  const titleId = useId();
  const panel = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    uiStore.set((prev) => {
      return { ...prev, openDialogs: prev.openDialogs + 1 };
    });
    const first = panel.current?.querySelector<HTMLElement>(FOCUSABLE);
    (first ?? panel.current)?.focus();
    return () => {
      uiStore.set((prev) => {
        return { ...prev, openDialogs: Math.max(0, prev.openDialogs - 1) };
      });
      opener?.focus();
    };
  }, []);

  const onKeyDown = (e: React.KeyboardEvent<HTMLDivElement>) => {
    if (e.key === "Escape") {
      e.stopPropagation();
      if (dismissible) {
        onClose();
      }
      return;
    }
    if (e.key !== "Tab" || !panel.current) {
      return;
    }
    const items = [...panel.current.querySelectorAll<HTMLElement>(FOCUSABLE)];
    if (items.length === 0) {
      e.preventDefault();
      return;
    }
    const firstItem = items[0];
    const lastItem = items[items.length - 1];
    if (e.shiftKey && document.activeElement === firstItem) {
      e.preventDefault();
      lastItem.focus();
    } else if (!e.shiftKey && document.activeElement === lastItem) {
      e.preventDefault();
      firstItem.focus();
    }
  };

  const flight = typeof leaving === "object" ? leaving : null;
  const overlayClasses = ["modal-overlay", leaving ? "leaving" : "", flight ? "flying" : ""].join(
    " ",
  );

  return (
    <div
      className={cx(styles, baseStyles, homeStyles, overlayClasses)}
      onClick={(e) => {
        if (dismissible && e.target === e.currentTarget) {
          onClose();
        }
      }}
    >
      <div
        ref={panel}
        className={cx(styles, baseStyles, homeStyles, wide ? "modal wide" : "modal")}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        tabIndex={-1}
        onKeyDown={onKeyDown}
        style={
          flight
            ? { transform: `translate(${flight.dx}px, ${flight.dy}px) scale(${flight.scale})` }
            : undefined
        }
      >
        <h3 id={titleId}>{title}</h3>
        {children}
        {actions && (
          <div className={cx(styles, baseStyles, homeStyles, "modal-actions")}>{actions}</div>
        )}
      </div>
    </div>
  );
}
