import { cx } from "../../cx";
import baseStyles from "../../styles/base.module.css";
import styles from "./export.module.css";
import { useEffect, useState } from "react";
import { t, type TextKey } from "../../i18n";
import { api } from "../../ipc";
import { describeError, patchUi, run } from "../../store";
import type { ExportAspectRatio, ExportCanvas, SessionView } from "../../types";
import { Dialog, type DialogExit } from "../chrome/Dialog";
import { InfoTip } from "../chrome/InfoTip";
import { alreadyNormalized } from "./audioNormalization";
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
const FADE_MS = 160;
const FLIGHT_MS = 450;

/** False when the user asked the system for reduced motion, or there is no way to ask. */
function motionEnabled(): boolean {
  return (
    typeof window.matchMedia === "function" &&
    !window.matchMedia("(prefers-reduced-motion: reduce)").matches
  );
}

/** The exit that carries the dialog to the export queue button, or a fade without one. */
function flightToQueue(): DialogExit {
  const panel = document.querySelector<HTMLElement>('[role="dialog"]');
  const target = document.querySelector<HTMLElement>('[data-fly-target="export-queue"]');
  if (!panel || !target) {
    return "fade";
  }
  const from = panel.getBoundingClientRect();
  const to = target.getBoundingClientRect();
  return {
    dx: to.left + to.width / 2 - (from.left + from.width / 2),
    dy: to.top + to.height / 2 - (from.top + from.height / 2),
    scale: Math.min(0.2, Math.max(0.03, to.width / Math.max(1, from.width))),
  };
}

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
 *  directly, with no further file picker. With `clipIds` it exports every one of those clips as
 *  its own video: the name field becomes an optional prefix and each file is named after its clip. */
export function ExportDialog({
  session,
  clipIds = null,
}: {
  session: SessionView;
  /** Set to export each of these clips as its own video, instead of the whole sequence. */
  clipIds?: number[] | null;
}) {
  const batch = clipIds !== null;
  const sequence = session.project.sequences[session.project.active_sequence];
  const settings = sequence.export_settings;
  const [fileName, setFileName] = useState(batch ? "" : sequence.name);
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
  const [storedNormalize, setStoredNormalize] = useState(true);
  // `null` until the user touches the checkbox: then the sound already normalized on the clips
  // turns the export's own normalization off by default.
  const [normalizeChoice, setNormalizeChoice] = useState<boolean | null>(null);
  const [existing, setExisting] = useState(0);
  const [ifExists, setIfExists] = useState<"rename" | "replace">("rename");
  const [leaving, setLeaving] = useState<DialogExit | null>(null);
  const [error, setError] = useState("");
  const measured = measuredLufs(session);
  const already = alreadyNormalized(session, lufs, clipIds);
  const normalize = normalizeChoice ?? (storedNormalize && already !== true);
  const lufsValid = !normalize || (Number.isFinite(lufs) && lufs >= MIN_LUFS && lufs <= MAX_LUFS);
  const emptyTimeline = canvasLoaded && canvas === null;
  const ready = (batch || fileName.trim() !== "") && folder.trim() !== "" && !emptyTimeline;
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
  const picture = exportOptions(quality, customMbps, maxHeight);
  const options = normalize ? picture : { ...picture, normalize: false };
  const summary = canvas && bitrateValid ? summarize(canvas, picture) : null;
  const someStepDisabled = QUALITIES.some(([value]) => {
    return stepDisabled(value);
  });

  useEffect(() => {
    void run(api.getNormalizeAudio()).then((value) => {
      if (typeof value === "boolean") {
        setStoredNormalize(value);
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
    api.exportCanvas(aspect, clipIds ?? undefined).then(
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
  }, [aspect, clipIds]);

  // Whether the file(s) about to be written already exist, looked up as the name or folder is
  // typed so the choice is made here rather than in a question after Export.
  useEffect(() => {
    if (!ready) {
      return;
    }
    let stale = false;
    const timer = setTimeout(() => {
      const lookup = clipIds
        ? api.exportClipsTargets(clipIds, folder, fileName)
        : api.exportTarget(folder, fileName).then((target) => {
            return [target];
          });
      lookup.then(
        (targets) => {
          if (!stale) {
            setExisting(
              targets.filter((target) => {
                return target.exists;
              }).length,
            );
          }
        },
        () => {
          if (!stale) {
            setExisting(0);
          }
        },
      );
    }, 250);
    return () => {
      stale = true;
      clearTimeout(timer);
    };
  }, [ready, folder, fileName, clipIds]);
  const existingShown = ready ? existing : 0;

  const close = () => {
    patchUi({ exportDialogOpen: false, exportClipIds: null });
  };

  /** Plays the exit and waits for it; with reduced motion, or no motion at all, it is instant. */
  const leave = async (exit: DialogExit) => {
    if (!motionEnabled()) {
      return;
    }
    setLeaving(exit);
    await new Promise((resolve) => {
      setTimeout(resolve, exit === "fade" ? FADE_MS : FLIGHT_MS);
    });
  };

  const dismiss = async () => {
    if (leaving) {
      return;
    }
    if (motionEnabled()) {
      await leave("fade");
    }
    close();
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
    if (leaving) {
      return;
    }
    let target: { path: string; exists: boolean } | null = null;
    try {
      if (clipIds) {
        await api.exportClipsTargets(clipIds, folder, fileName);
      } else {
        target = await api.exportTarget(folder, fileName);
      }
    } catch (e) {
      setError(describeError(e));
      return;
    }
    const rename = existingShown > 0 && ifExists === "rename";
    // The job is queued while the dialog flies to the queue button.
    const queued = (async () => {
      if (normalizeChoice !== null) {
        await run(api.setNormalizeAudio(normalizeChoice));
      }
      await run(api.setExportSettings(Number.isFinite(lufs) ? lufs : settings.target_lufs, aspect));
      if (clipIds) {
        await run(api.startExportClips(clipIds, folder, fileName, rename, options));
      } else if (target) {
        await run(api.startExport(target.path, rename, options));
      }
    })();
    await leave(flightToQueue());
    close();
    patchUi({ exportQueueOpen: true });
    await queued;
    const prefs = await run(api.getPrefs());
    if (prefs && prefs.outputFolder !== folder) {
      await run(api.setPrefs({ ...prefs, outputFolder: folder }));
    }
  };

  return (
    <Dialog
      title={
        clipIds
          ? t("exportClipsTitle").replace("{n}", String(clipIds.length))
          : t("exportDialogTitle")
      }
      onClose={() => {
        void dismiss();
      }}
      leaving={leaving}
      actions={
        <>
          <button
            className={cx(styles, baseStyles, "icon")}
            onClick={() => {
              void dismiss();
            }}
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
        <span>{batch ? t("exportFilePrefix") : t("exportFileName")}</span>
        <input
          type="text"
          value={fileName}
          placeholder={batch ? t("exportFilePrefixPlaceholder") : undefined}
          onChange={(e) => {
            setFileName(e.target.value);
            setError("");
          }}
        />
        <span className={cx(styles, baseStyles, "muted")}>{batch ? "" : ".mp4"}</span>
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
      {existingShown > 0 && (
        <>
          <p
            className={cx(styles, baseStyles, "dialog-warning")}
            role="status"
          >
            {batch
              ? t("exportExistsMany").replace("{n}", String(existingShown))
              : t("exportExistsOne")}
          </p>
          <label className={cx(styles, baseStyles, "dialog-field")}>
            <span>{t("exportIfExists")}</span>
            <select
              value={ifExists}
              onChange={(e) => {
                setIfExists(e.target.value as "rename" | "replace");
              }}
            >
              <option value="rename">{t("exportKeepBoth")}</option>
              <option value="replace">{t("exportReplace")}</option>
            </select>
          </label>
        </>
      )}
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
            setNormalizeChoice(e.target.checked);
          }}
        />
        <span>{t("normalizeAudio")}</span>
      </label>
      {already === true && (
        <p className={cx(styles, baseStyles, "muted")}>
          {t("normalizeAlready").replace("{lufs}", String(lufs))}
        </p>
      )}
      {!normalize && already !== true && (
        <p className={cx(styles, baseStyles, "muted")}>{t("normalizeAudioHint")}</p>
      )}
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
            {batch ? t("exportEstimateTotal") : t("exportEstimate")} {formatSize(summary.sizeMb)}
          </span>
        </p>
      )}
      {emptyTimeline && (
        <p
          className={cx(styles, baseStyles, "dialog-error")}
          role="alert"
        >
          {batch ? t("exportNoClips") : t("exportEmptyTimeline")}
        </p>
      )}
      {!emptyTimeline && Object.keys(picture).length === 0 && (
        <p className={cx(styles, baseStyles, "muted")}>{t("exportKeepsBitrate")}</p>
      )}
    </Dialog>
  );
}
