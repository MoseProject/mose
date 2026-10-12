import { cx } from "../../cx";
import baseStyles from "../../styles/base.module.css";
import styles from "./timeline.module.css";
import { createPortal } from "react-dom";
import { useEffect, useRef, useState } from "react";
import { t } from "../../i18n";
import { api } from "../../ipc";
import { run } from "../../store";
import { MAX_TRACK_VOLUME_DB, MIN_TRACK_VOLUME_DB, type Track, type TrackMix } from "../../types";
import { Icon } from "../chrome/Icon";
import { Slider } from "../inspector/controls";

/** "L40", "C", "R40": the pan as a mixer prints it. */
export function formatPan(pan: number): string {
  const percent = Math.round(Math.abs(pan) * 100);
  if (percent === 0) {
    return "C";
  }
  return `${pan < 0 ? "L" : "R"}${percent}`;
}

export function formatVolume(volumeDb: number): string {
  return `${volumeDb > 0 ? "+" : ""}${volumeDb.toFixed(1)} dB`;
}

const isNeutral = (mix: TrackMix) => {
  return mix.volume_db === 0 && mix.pan === 0;
};

/** The track header's mixer button: opens a small panel with the track's fader and pan. Lit while
 *  the mix is not neutral, so a track that is quieter or off-centre can be spotted at a glance. */
export function TrackMixButton({ track }: { track: Track }) {
  const [open, setOpen] = useState(false);
  const [anchor, setAnchor] = useState<DOMRect | null>(null);
  const button = useRef<HTMLButtonElement>(null);
  const panel = useRef<HTMLDivElement>(null);
  const mix = track.mix ?? { volume_db: 0, pan: 0 };

  useEffect(() => {
    if (!open) {
      return;
    }
    const onPointerDown = (e: PointerEvent) => {
      const target = e.target as Node;
      if (!panel.current?.contains(target) && !button.current?.contains(target)) {
        setOpen(false);
      }
    };
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        setOpen(false);
        button.current?.focus();
      }
    };
    document.addEventListener("pointerdown", onPointerDown);
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("pointerdown", onPointerDown);
      document.removeEventListener("keydown", onKeyDown);
    };
  }, [open]);

  const set = (next: TrackMix) => {
    return run(api.setTrackMix(track.id, next.volume_db, next.pan));
  };

  return (
    <>
      <button
        ref={button}
        className={cx(styles, baseStyles, `icon ${isNeutral(mix) ? "" : "active"}`)}
        title={t("trackMix")}
        aria-label={t("trackMix")}
        aria-expanded={open}
        data-testid={`track-mix-${track.id}`}
        onClick={() => {
          setAnchor(button.current?.getBoundingClientRect() ?? null);
          setOpen(!open);
        }}
      >
        <Icon name="gauge" />
      </button>
      {open &&
        anchor &&
        createPortal(
          <div
            ref={panel}
            className={cx(styles, baseStyles, "track-mix-panel")}
            role="dialog"
            aria-label={t("trackMix")}
            style={{ left: anchor.left, top: anchor.bottom + 4 }}
          >
            <Slider
              label={t("trackVolume")}
              value={mix.volume_db}
              min={MIN_TRACK_VOLUME_DB}
              max={MAX_TRACK_VOLUME_DB}
              step={0.5}
              format={formatVolume}
              onCommit={(volume_db) => {
                return set({ ...mix, volume_db });
              }}
            />
            <Slider
              label={t("trackPan")}
              value={mix.pan}
              min={-1}
              max={1}
              step={0.05}
              format={formatPan}
              onCommit={(pan) => {
                return set({ ...mix, pan });
              }}
            />
            <button
              disabled={isNeutral(mix)}
              data-testid="track-mix-reset"
              onClick={() => {
                return set({ volume_db: 0, pan: 0 });
              }}
            >
              {t("resetSection")}
            </button>
          </div>,
          document.body,
        )}
    </>
  );
}
