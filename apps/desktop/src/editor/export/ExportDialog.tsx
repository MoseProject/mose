import { cx } from "../../cx";
import baseStyles from "../../styles/base.module.css";
import styles from "./export.module.css";
import { useEffect, useState } from "react";
import { t, type TextKey } from "../../i18n";
import { api, exportActiveSequence } from "../../ipc";
import { askConfirm } from "../../notifications";
import { patchUi, run } from "../../store";
import type { ExportAspectRatio, SessionView } from "../../types";
import { Dialog } from "../chrome/Dialog";

const ASPECTS: [ExportAspectRatio, TextKey][] = [
  ["Original", "aspectOriginal"],
  ["Landscape", "aspectLandscape"],
  ["Portrait", "aspectPortrait"],
  ["Square", "aspectSquare"],
];
const LOUDNESS_PRESETS = [-14, -16, -23];
const MIN_LUFS = -40;
const MAX_LUFS = -5;

/** Loudness of the first clip's source in the active sequence, when it has been measured. */
function measuredLufs(session: SessionView): number | null {
  const sequence = session.project.sequences[session.project.active_sequence];
  for (const track of sequence.timeline.tracks) {
    for (const clip of track.clips) {
      const asset = session.project.media_library.find((item) => {
        return item.id === clip.asset_id;
      });
      if (asset?.loudness) {
        return asset.loudness.integrated_lufs;
      }
    }
  }
  return null;
}

/** Export options asked before the file picker: aspect ratio, loudness target and encoder. */
export function ExportDialog({ session }: { session: SessionView }) {
  const settings = session.project.sequences[session.project.active_sequence].export_settings;
  const [aspect, setAspect] = useState<ExportAspectRatio>(settings.aspect_ratio);
  const [lufs, setLufs] = useState(settings.target_lufs);
  const [gpu, setGpu] = useState("auto");
  const [normalize, setNormalize] = useState(true);
  const measured = measuredLufs(session);
  const lufsValid = !normalize || (Number.isFinite(lufs) && lufs >= MIN_LUFS && lufs <= MAX_LUFS);

  useEffect(() => {
    void run(api.getNormalizeAudio()).then((value) => {
      if (typeof value === "boolean") {
        setNormalize(value);
      }
    });
    void run(api.getGpuEncoderPreference()).then((value) => {
      if (value) {
        setGpu(value);
      }
    });
  }, []);

  const close = () => {
    patchUi({ exportDialogOpen: false });
  };

  const start = async () => {
    if (!lufsValid) {
      return;
    }
    close();
    patchUi({ exportQueueOpen: true });
    await run(api.setGpuEncoderPreference(gpu));
    await run(api.setNormalizeAudio(normalize));
    await run(api.setExportSettings(Number.isFinite(lufs) ? lufs : settings.target_lufs, aspect));
    await run(
      exportActiveSequence(() => {
        return askConfirm({
          title: t("confirmOverwriteTitle"),
          message: t("exportOverwriteConfirm"),
          confirmLabel: t("confirmOverwriteAction"),
          cancelLabel: t("confirmKeepBoth"),
        });
      }),
    );
  };

  return (
    <Dialog
      title={t("exportDialogTitle")}
      onClose={close}
      actions={
        <>
          <button
            className={cx(styles, baseStyles, "icon")}
            onClick={close}
          >
            {t("cancel")}
          </button>
          <button
            className={cx(styles, baseStyles, "primary")}
            disabled={!lufsValid}
            onClick={() => {
              void start();
            }}
          >
            {t("exportChooseFile")}
          </button>
        </>
      }
    >
      <fieldset className={cx(styles, baseStyles, "dialog-group")}>
        <legend>{t("aspectRatio")}</legend>
        <div
          className={cx(styles, baseStyles, "segmented")}
          role="radiogroup"
          aria-label={t("aspectRatio")}
        >
          {ASPECTS.map(([value, label]) => {
            return (
              <button
                key={value}
                role="radio"
                aria-checked={aspect === value}
                className={cx(styles, baseStyles, aspect === value ? "active" : "")}
                onClick={() => {
                  setAspect(value);
                }}
              >
                {t(label)}
              </button>
            );
          })}
        </div>
      </fieldset>
      <label className={cx(styles, baseStyles, "dialog-check")}>
        <input
          type="checkbox"
          checked={normalize}
          onChange={(e) => {
            setNormalize(e.target.checked);
          }}
        />
        <span>{t("normalizeAudio")}</span>
      </label>
      {!normalize && <p className={cx(styles, baseStyles, "muted")}>{t("normalizeAudioHint")}</p>}
      <fieldset
        className={cx(styles, baseStyles, "dialog-group")}
        disabled={!normalize}
      >
        <legend>{t("loudnessTarget")}</legend>
        <div
          className={cx(styles, baseStyles, "segmented")}
          role="radiogroup"
          aria-label={t("loudnessTarget")}
        >
          {LOUDNESS_PRESETS.map((preset) => {
            return (
              <button
                key={preset}
                role="radio"
                aria-checked={lufs === preset}
                className={cx(styles, baseStyles, lufs === preset ? "active" : "")}
                onClick={() => {
                  setLufs(preset);
                }}
              >
                {preset} LUFS
              </button>
            );
          })}
        </div>
        <label className={cx(styles, baseStyles, "dialog-field")}>
          <span>{t("loudnessCustom")}</span>
          <input
            type="number"
            min={MIN_LUFS}
            max={MAX_LUFS}
            step={0.5}
            value={Number.isFinite(lufs) ? lufs : ""}
            aria-invalid={!lufsValid}
            onChange={(e) => {
              setLufs(e.target.value === "" ? Number.NaN : Number(e.target.value));
            }}
          />
          <span className={cx(styles, baseStyles, "muted")}>LUFS</span>
        </label>
        {!lufsValid && (
          <p
            className={cx(styles, baseStyles, "dialog-error")}
            role="alert"
          >
            {t("loudnessRange")}
          </p>
        )}
        {measured !== null && (
          <p className={cx(styles, baseStyles, "muted")}>
            {t("loudnessMeasured")}: {measured.toFixed(1)} LUFS
          </p>
        )}
      </fieldset>
      <label className={cx(styles, baseStyles, "dialog-field")}>
        <span>{t("gpuEncoder")}</span>
        <select
          value={gpu}
          onChange={(e) => {
            setGpu(e.target.value);
          }}
        >
          <option value="auto">Auto</option>
          <option value="cpu">CPU</option>
          <option value="nvenc">NVENC</option>
          <option value="quick_sync">Quick Sync</option>
          <option value="amf">AMF</option>
          <option value="vaapi">VAAPI</option>
          <option value="video_toolbox">VideoToolbox</option>
        </select>
      </label>
      <p className={cx(styles, baseStyles, "muted")}>{t("exportKeepsBitrate")}</p>
    </Dialog>
  );
}
