import { cx } from "../../cx";
import baseStyles from "../../styles/base.module.css";
import styles from "./export.module.css";
import { useEffect, useState } from "react";
import { t, type TextKey } from "../../i18n";
import { api } from "../../ipc";
import { askConfirm } from "../../notifications";
import { describeError, patchUi, run } from "../../store";
import type { ExportAspectRatio, ExportCanvas, SessionView } from "../../types";
import { Dialog } from "../chrome/Dialog";
import {
  customBitrateValid,
  exportOptions,
  formatSize,
  MAX_MBPS,
  MIN_MBPS,
  RESOLUTION_HEIGHTS,
  summarize,
  type QualityChoice,
} from "./exportPlan";

const ASPECTS: [ExportAspectRatio, TextKey][] = [
  ["Original", "aspectOriginal"],
  ["Landscape", "aspectLandscape"],
  ["Portrait", "aspectPortrait"],
  ["Square", "aspectSquare"],
];
const QUALITIES: [QualityChoice, TextKey][] = [
  ["source", "exportQualitySource"],
  ["high", "exportQualityHigh"],
  ["medium", "exportQualityMedium"],
  ["low", "exportQualityLow"],
  ["custom", "exportQualityCustom"],
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

/** Export settings, the way an editor's export panel lays them out: where the file goes and what
 *  it is called, quality, size and aspect, audio loudness and encoder. Exporting queues the job
 *  directly, with no further file picker. */
export function ExportDialog({ session }: { session: SessionView }) {
  const sequence = session.project.sequences[session.project.active_sequence];
  const settings = sequence.export_settings;
  const [fileName, setFileName] = useState(sequence.name);
  const [folder, setFolder] = useState("");
  const [aspect, setAspect] = useState<ExportAspectRatio>(settings.aspect_ratio);
  const [quality, setQuality] = useState<QualityChoice>("source");
  const [customMbps, setCustomMbps] = useState(10);
  const [maxHeight, setMaxHeight] = useState<number | null>(null);
  const [canvas, setCanvas] = useState<ExportCanvas | null>(null);
  const [lufs, setLufs] = useState(settings.target_lufs);
  const [gpu, setGpu] = useState("auto");
  const [normalize, setNormalize] = useState(true);
  const [error, setError] = useState("");
  const measured = measuredLufs(session);
  const lufsValid = !normalize || (Number.isFinite(lufs) && lufs >= MIN_LUFS && lufs <= MAX_LUFS);
  const bitrateValid = quality !== "custom" || customBitrateValid(customMbps);
  const options = exportOptions(quality, customMbps, maxHeight);
  const summary = canvas && bitrateValid ? summarize(canvas, options) : null;

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
    void run(api.getPrefs()).then((prefs) => {
      if (prefs) {
        setFolder((current) => {
          return current || prefs.outputFolder;
        });
      }
    });
  }, []);

  useEffect(() => {
    let stale = false;
    api.exportCanvas(aspect).then(
      (value) => {
        if (!stale) {
          setCanvas(value);
        }
      },
      () => {
        if (!stale) {
          setCanvas(null);
        }
      },
    );
    return () => {
      stale = true;
    };
  }, [aspect]);

  const close = () => {
    patchUi({ exportDialogOpen: false });
  };

  const browse = async () => {
    const picked = await run(api.pickPath("folder"));
    if (picked) {
      setFolder(picked);
      setError("");
    }
  };

  const start = async () => {
    if (!lufsValid || !bitrateValid) {
      return;
    }
    let target: { path: string; exists: boolean };
    try {
      target = await api.exportTarget(folder, fileName);
    } catch (e) {
      setError(describeError(e));
      return;
    }
    close();
    patchUi({ exportQueueOpen: true });
    await run(api.setGpuEncoderPreference(gpu));
    await run(api.setNormalizeAudio(normalize));
    await run(api.setExportSettings(Number.isFinite(lufs) ? lufs : settings.target_lufs, aspect));
    const replace = target.exists
      ? await askConfirm({
          title: t("confirmOverwriteTitle"),
          message: t("exportOverwriteConfirm"),
          confirmLabel: t("confirmOverwriteAction"),
          cancelLabel: t("confirmKeepBoth"),
        })
      : true;
    await run(api.startExport(target.path, target.exists && !replace, options));
    const prefs = await run(api.getPrefs());
    if (prefs && prefs.outputFolder !== folder) {
      await run(api.setPrefs({ ...prefs, outputFolder: folder }));
    }
  };

  return (
    <Dialog
      title={t("exportDialogTitle")}
      onClose={close}
      wide
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
            disabled={!lufsValid || !bitrateValid}
            onClick={() => {
              void start();
            }}
          >
            {t("exportChooseFile")}
          </button>
        </>
      }
    >
      <label className={cx(styles, baseStyles, "dialog-field")}>
        <span>{t("exportFileName")}</span>
        <input
          type="text"
          value={fileName}
          onChange={(e) => {
            setFileName(e.target.value);
            setError("");
          }}
        />
        <span className={cx(styles, baseStyles, "muted")}>.mp4</span>
      </label>
      <div className={cx(styles, baseStyles, "dialog-field")}>
        <span id="export-folder-label">{t("exportFolder")}</span>
        <input
          type="text"
          aria-labelledby="export-folder-label"
          value={folder}
          placeholder={t("exportFolderPlaceholder")}
          onChange={(e) => {
            setFolder(e.target.value);
            setError("");
          }}
        />
        <button
          onClick={() => {
            void browse();
          }}
        >
          {t("browse")}
        </button>
      </div>
      {error && (
        <p
          className={cx(styles, baseStyles, "dialog-error")}
          role="alert"
        >
          {error}
        </p>
      )}
      <fieldset className={cx(styles, baseStyles, "dialog-group")}>
        <legend>{t("exportQuality")}</legend>
        <div
          className={cx(styles, baseStyles, "segmented")}
          role="radiogroup"
          aria-label={t("exportQuality")}
        >
          {QUALITIES.map(([value, label]) => {
            return (
              <button
                key={value}
                role="radio"
                aria-checked={quality === value}
                className={cx(styles, baseStyles, quality === value ? "active" : "")}
                onClick={() => {
                  setQuality(value);
                }}
              >
                {t(label)}
              </button>
            );
          })}
        </div>
        {quality === "custom" && (
          <label className={cx(styles, baseStyles, "dialog-field")}>
            <span>{t("exportBitrate")}</span>
            <input
              type="number"
              min={MIN_MBPS}
              max={MAX_MBPS}
              step={0.5}
              value={Number.isFinite(customMbps) ? customMbps : ""}
              aria-invalid={!bitrateValid}
              onChange={(e) => {
                setCustomMbps(e.target.value === "" ? Number.NaN : Number(e.target.value));
              }}
            />
            <span className={cx(styles, baseStyles, "muted")}>Mbps</span>
          </label>
        )}
        {!bitrateValid && (
          <p
            className={cx(styles, baseStyles, "dialog-error")}
            role="alert"
          >
            {t("exportBitrateRange")}
          </p>
        )}
      </fieldset>
      <fieldset className={cx(styles, baseStyles, "dialog-group")}>
        <legend>{t("exportResolution")}</legend>
        <div
          className={cx(styles, baseStyles, "segmented")}
          role="radiogroup"
          aria-label={t("exportResolution")}
        >
          {[
            null,
            ...RESOLUTION_HEIGHTS.filter((h) => {
              return canvas === null || h < canvas.height;
            }),
          ].map((height) => {
            return (
              <button
                key={height ?? "source"}
                role="radio"
                aria-checked={maxHeight === height}
                className={cx(styles, baseStyles, maxHeight === height ? "active" : "")}
                onClick={() => {
                  setMaxHeight(height);
                }}
              >
                {height === null ? t("exportResolutionSource") : `${height}p`}
              </button>
            );
          })}
        </div>
      </fieldset>
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
      {summary && canvas && (
        <p
          className={cx(styles, baseStyles, "export-summary")}
          data-testid="export-summary"
        >
          <span>{t("exportSummary")}</span>
          <span className={cx(styles, baseStyles, "mono")}>
            {summary.width}x{summary.height} · {summary.bitrateMbps.toFixed(1)} Mbps ·{" "}
            {t("exportEstimate")} {formatSize(summary.sizeMb)}
          </span>
        </p>
      )}
      <p className={cx(styles, baseStyles, "muted")}>{t("exportKeepsBitrate")}</p>
    </Dialog>
  );
}
