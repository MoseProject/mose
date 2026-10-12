import { cx } from "../../cx";
import baseStyles from "../../styles/base.module.css";
import styles from "./timeline.module.css";
import { createPortal } from "react-dom";
import { useEffect, useRef, useState } from "react";
import { t } from "../../i18n";
import { api } from "../../ipc";
import { run } from "../../store";
import {
  MAX_TRACK_COMP_RATIO,
  MAX_TRACK_EQ_DB,
  MAX_TRACK_HIGHPASS_HZ,
  MAX_TRACK_LOWPASS_HZ,
  MAX_TRACK_VOLUME_DB,
  MIN_TRACK_COMP_THRESHOLD_DB,
  MIN_TRACK_LOWPASS_HZ,
  MIN_TRACK_VOLUME_DB,
  NEUTRAL_TRACK_MIX,
  type Track,
  type TrackMix,
} from "../../types";
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

export function formatEq(gainDb: number): string {
  return `${gainDb > 0 ? "+" : ""}${gainDb.toFixed(1)} dB`;
}

export function formatCorner(hz: number): string {
  if (hz <= 0) {
    return t("trackFilterOff");
  }
  return hz >= 1000 ? `${(hz / 1000).toFixed(1)} kHz` : `${Math.round(hz)} Hz`;
}

export function formatRatio(ratio: number): string {
  return ratio <= 1 ? t("trackCompOff") : `${ratio.toFixed(1)}:1`;
}

const isNeutral = (mix: TrackMix) => {
  return (Object.keys(NEUTRAL_TRACK_MIX) as (keyof TrackMix)[]).every((key) => {
    // A threshold means nothing while the ratio is off.
    return key === "comp_threshold_db" ? true : mix[key] === NEUTRAL_TRACK_MIX[key];
  });
};

/** The track header's mixer button: opens a small panel with the track's fader and pan. Lit while
 *  the mix is not neutral, so a track that is quieter or off-centre can be spotted at a glance. */
export function TrackMixButton({ track }: { track: Track }) {
  const [open, setOpen] = useState(false);
  const [anchor, setAnchor] = useState<DOMRect | null>(null);
  const button = useRef<HTMLButtonElement>(null);
  const panel = useRef<HTMLDivElement>(null);
  const mix = { ...NEUTRAL_TRACK_MIX, ...track.mix };

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
    return run(api.setTrackMix(track.id, next));
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
            <Slider
              label={t("trackEqLow")}
              value={mix.eq_low_db}
              min={-MAX_TRACK_EQ_DB}
              max={MAX_TRACK_EQ_DB}
              step={0.5}
              format={formatEq}
              onCommit={(eq_low_db) => {
                return set({ ...mix, eq_low_db });
              }}
            />
            <Slider
              label={t("trackEqMid")}
              value={mix.eq_mid_db}
              min={-MAX_TRACK_EQ_DB}
              max={MAX_TRACK_EQ_DB}
              step={0.5}
              format={formatEq}
              onCommit={(eq_mid_db) => {
                return set({ ...mix, eq_mid_db });
              }}
            />
            <Slider
              label={t("trackEqHigh")}
              value={mix.eq_high_db}
              min={-MAX_TRACK_EQ_DB}
              max={MAX_TRACK_EQ_DB}
              step={0.5}
              format={formatEq}
              onCommit={(eq_high_db) => {
                return set({ ...mix, eq_high_db });
              }}
            />
            <Slider
              label={t("trackHighpass")}
              value={mix.highpass_hz}
              min={0}
              max={MAX_TRACK_HIGHPASS_HZ}
              step={10}
              format={formatCorner}
              onCommit={(highpass_hz) => {
                return set({ ...mix, highpass_hz });
              }}
            />
            <Slider
              label={t("trackLowpass")}
              value={mix.lowpass_hz > 0 ? mix.lowpass_hz : MAX_TRACK_LOWPASS_HZ}
              min={MIN_TRACK_LOWPASS_HZ}
              max={MAX_TRACK_LOWPASS_HZ}
              step={100}
              format={(hz) => {
                return formatCorner(hz >= MAX_TRACK_LOWPASS_HZ ? 0 : hz);
              }}
              onCommit={(hz) => {
                return set({ ...mix, lowpass_hz: hz >= MAX_TRACK_LOWPASS_HZ ? 0 : hz });
              }}
            />
            <Slider
              label={t("trackCompThreshold")}
              value={mix.comp_threshold_db}
              min={MIN_TRACK_COMP_THRESHOLD_DB}
              max={0}
              step={1}
              format={(db) => {
                return `${db.toFixed(0)} dB`;
              }}
              onCommit={(comp_threshold_db) => {
                return set({ ...mix, comp_threshold_db });
              }}
            />
            <Slider
              label={t("trackCompRatio")}
              value={mix.comp_ratio}
              min={1}
              max={MAX_TRACK_COMP_RATIO}
              step={0.5}
              format={formatRatio}
              onCommit={(comp_ratio) => {
                return set({ ...mix, comp_ratio });
              }}
            />
            <button
              disabled={isNeutral(mix)}
              data-testid="track-mix-reset"
              onClick={() => {
                return set(NEUTRAL_TRACK_MIX);
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
