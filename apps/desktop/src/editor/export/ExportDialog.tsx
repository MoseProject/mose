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
import { InfoTip } from "../chrome/InfoTip";
import {
  customBitrateValid,
  isUpscale,
  exportOptions,
  formatSize,
  MAX_MBPS,
  MIN_MBPS,
  QUALITY_MBPS,
  RESOLUTION_HEIGHTS,
  summarize,
  type QualityChoice,
} from "./exportPlan";

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
 *  it is called, quality, size, audio loudness. Exporting queues the job
 *  directly, with no further file picker. */
export function ExportDialog({ session }: { session: SessionView }) {
  const sequence = session.project.sequences[session.project.active_sequence];
  const settings = sequence.export_settings;
  const [fileName, setFileName] = useState(sequence.name);
  const [folder, setFolder] = useState("");
  // The picture keeps the aspect ratio the project is set to; it is not chosen per export.
  const aspect: ExportAspectRatio = settings.aspect_ratio;
  const [chosenQuality, setQuality] = useState<QualityChoice>("source");
  const [customMbps, setCustomMbps] = useState(10);
  const [maxHeight, setMaxHeight] = useState<number | null>(null);
  const [canvas, setCanvas] = useState<ExportCanvas | null>(null);
  const [canvasLoaded, setCanvasLoaded] = useState(false);
  const [lufsCustom, setLufsCustom] = useState(!LOUDNESS_PRESETS.includes(settings.target_lufs));
  const [lufs, setLufs] = useState(settings.target_lufs);
  const [normalize, setNormalize] = useState(true);
  const [error, setError] = useState("");
  const measured = measuredLufs(session);
  const lufsValid = !normalize || (Number.isFinite(lufs) && lufs >= MIN_LUFS && lufs <= MAX_LUFS);
  const emptyTimeline = canvasLoaded && canvas === null;
  const ready = fileName.trim() !== "" && folder.trim() !== "" && !emptyTimeline;
  // A fixed step above the bitrate the picture already has would only make the file bigger.
  const sourceMbps = canvas
    ? summarize(canvas, { maxHeight: maxHeight ?? undefined }).bitrateMbps
    : null;
  const stepDisabled = (value: QualityChoice) => {
    return (
      value !== "source" &&
      value !== "custom" &&
      sourceMbps !== null &&
      QUALITY_MBPS[value] > sourceMbps
    );
  };
  // A step the current picture size made too high falls back to the source quality.
  const quality = stepDisabled(chosenQuality) ? "source" : chosenQuality;
  const bitrateValid = quality !== "custom" || customBitrateValid(customMbps);
  const options = exportOptions(quality, customMbps, maxHeight);
  const summary = canvas && bitrateValid ? summarize(canvas, options) : null;
  const someStepDisabled = QUALITIES.some(([value]) => {
    return stepDisabled(value);
  });

  useEffect(() => {
    void run(api.getNormalizeAudio()).then((value) => {
      if (typeof value === "boolean") {
        setNormalize(value);
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
          setCanvasLoaded(true);
        }
      },
      () => {
        if (!stale) {
          setCanvas(null);
          setCanvasLoaded(true);
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
    if (!lufsValid || !bitrateValid || !ready) {
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
            disabled={!lufsValid || !bitrateValid || !ready}
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
      <label className={cx(styles, baseStyles, "dialog-field")}>
        <span>{t("exportQuality")}</span>
        <select
          value={quality}
          onChange={(e) => {
            setQuality(e.target.value as QualityChoice);
          }}
        >
          {QUALITIES.map(([value, label]) => {
            return (
              <option
                key={value}
                value={value}
                disabled={stepDisabled(value)}
              >
                {t(label)}
              </option>
            );
          })}
        </select>
      </label>
      {someStepDisabled && (
        <p className={cx(styles, baseStyles, "muted")}>{t("exportQualityAboveSource")}</p>
      )}
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
      <label className={cx(styles, baseStyles, "dialog-field")}>
        <span>{t("exportResolution")}</span>
        <select
          value={maxHeight === null ? "source" : String(maxHeight)}
          onChange={(e) => {
            setMaxHeight(e.target.value === "source" ? null : Number(e.target.value));
          }}
        >
          <option value="source">
            {t("exportResolutionSource")}
            {canvas ? ` (${canvas.width}x${canvas.height}) · ${t("exportRecommended")}` : ""}
          </option>
          {RESOLUTION_HEIGHTS.filter((h) => {
            return canvas === null || h !== canvas.height;
          }).map((height) => {
            return (
              <option
                key={height}
                value={height}
              >
                {height}p{canvas && height > canvas.height ? ` · ${t("exportUpscale")}` : ""}
              </option>
            );
          })}
        </select>
        {isUpscale(canvas, maxHeight) && (
          <InfoTip
            label={t("exportUpscale")}
            text={t("exportUpscaleNote")}
          />
        )}
      </label>
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
        <label className={cx(styles, baseStyles, "dialog-field")}>
          <span>{t("loudnessTarget")}</span>
          <select
            value={lufsCustom ? "custom" : String(lufs)}
            onChange={(e) => {
              const custom = e.target.value === "custom";
              setLufsCustom(custom);
              if (!custom) {
                setLufs(Number(e.target.value));
              }
            }}
          >
            {LOUDNESS_PRESETS.map((preset) => {
              return (
                <option
                  key={preset}
                  value={preset}
                >
                  {preset} LUFS
                </option>
              );
            })}
            <option value="custom">{t("loudnessCustom")}</option>
          </select>
        </label>
        {lufsCustom && (
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
        )}
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
      {emptyTimeline && (
        <p
          className={cx(styles, baseStyles, "dialog-error")}
          role="alert"
        >
          {t("exportEmptyTimeline")}
        </p>
      )}
      {!emptyTimeline && Object.keys(options).length === 0 && (
        <p className={cx(styles, baseStyles, "muted")}>{t("exportKeepsBitrate")}</p>
      )}
    </Dialog>
  );
}
