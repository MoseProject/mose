import { useEffect, useState, type ReactNode } from "react";
import { localeFromPrefs, setLocale, t } from "../i18n";
import { api, onUpdateCheck } from "../ipc";
import { run } from "../store";
import type { ErrorReportingConsent, PathStatus, PrefsView, UpdateCheckEvent } from "../types";

function Group({ title, children }: { title: string; children: ReactNode }) {
  return (
    <fieldset className="settings-group">
      <legend>{title}</legend>
      {children}
    </fieldset>
  );
}

/** A path setting: typed or browsed, and checked against the disk so a wrong path shows up here
 *  rather than as a failure the first time the feature runs. */
function PathField({
  label,
  value,
  kind,
  extensions = [],
  onChange,
}: {
  label: string;
  value: string;
  kind: "file" | "folder";
  extensions?: string[];
  onChange: (value: string) => void;
}) {
  const [checked, setChecked] = useState<{ value: string; status: PathStatus } | null>(null);

  useEffect(() => {
    let current = true;
    void run(api.checkPath(value)).then((status) => {
      if (current && status) {
        setChecked({ value, status });
      }
    });
    return () => {
      current = false;
    };
  }, [value]);

  const status = checked && checked.value === value ? checked.status : null;
  const wrongKind =
    status !== null &&
    ((kind === "file" && status === "folder") || (kind === "folder" && status === "file"));
  return (
    <div className="path-field">
      <label>
        <span>{label}</span>
        <input
          value={value}
          aria-invalid={status === "missing" || wrongKind}
          onChange={(e) => {
            onChange(e.target.value);
          }}
        />
      </label>
      <button
        type="button"
        onClick={() => {
          void run(api.pickPath(kind, extensions)).then((picked) => {
            if (picked) {
              onChange(picked);
            }
          });
        }}
      >
        {t("browse")}
      </button>
      {status === "missing" && <span className="path-status bad">{t("pathMissing")}</span>}
      {wrongKind && <span className="path-status bad">{t("pathWrongKind")}</span>}
      {(status === "file" || status === "folder") && !wrongKind && (
        <span className="path-status ok">{t("pathOk")}</span>
      )}
    </div>
  );
}

export function Prefs() {
  const [prefs, setPrefs] = useState<PrefsView | null>(null);
  const [queuedReports, setQueuedReports] = useState(0);
  const [updateStatus, setUpdateStatus] = useState<UpdateCheckEvent | null>(null);
  const [gpu, setGpu] = useState("auto");
  const [savedTick, setSavedTick] = useState(0);

  useEffect(() => {
    void run(api.getPrefs()).then((value) => {
      return value && setPrefs(value);
    });
    void run(api.getGpuEncoderPreference()).then((value) => {
      return value && setGpu(value);
    });
    void run(api.getQueuedReportCount()).then((count) => {
      return setQueuedReports(count ?? 0);
    });
    const unlisten = onUpdateCheck((event) => {
      return setUpdateStatus(event);
    });
    return () => {
      void unlisten.then((stop) => {
        return stop();
      });
    };
  }, []);

  // The saved note shows for a moment after each change: settings apply as soon as they change.
  useEffect(() => {
    if (savedTick === 0) {
      return;
    }
    const timer = window.setTimeout(() => {
      setSavedTick(0);
    }, 1800);
    return () => {
      window.clearTimeout(timer);
    };
  }, [savedTick]);

  const update = (patch: Partial<PrefsView>) => {
    if (!prefs) {
      return;
    }
    const next = { ...prefs, ...patch };
    setPrefs(next);
    if (patch.locale) {
      setLocale(localeFromPrefs(patch.locale));
    }
    void run(api.setPrefs(next));
    setSavedTick(Date.now());
  };

  const setConsent = (consent: ErrorReportingConsent) => {
    if (!prefs) {
      return;
    }
    setPrefs({ ...prefs, errorReportingConsent: consent });
    void run(api.setErrorReportingConsent(consent));
    setSavedTick(Date.now());
  };

  if (!prefs) {
    return null;
  }
  return (
    <div
      className="prefs"
      data-testid="screen-prefs"
    >
      <Group title={t("settingsGeneral")}>
        <label>
          {t("language")}
          <select
            value={prefs.locale}
            onChange={(e) => {
              return update({ locale: e.target.value });
            }}
          >
            <option value="pt_br">Português</option>
            <option value="en">English</option>
          </select>
        </label>
        <label>
          {t("autosaveInterval")}
          <span>
            <input
              type="number"
              min={1}
              max={60}
              value={prefs.autosaveMinutes}
              onChange={(e) => {
                return update({ autosaveMinutes: Number(e.target.value) || 1 });
              }}
            />{" "}
            {t("minutes")}
          </span>
        </label>
      </Group>
      <Group title={t("settingsExport")}>
        <label>
          {t("defaultLufs")}
          <select
            value={prefs.lufsProfile}
            onChange={(e) => {
              return update({ lufsProfile: Number(e.target.value) });
            }}
          >
            <option value={0}>-14 LUFS | YouTube</option>
            <option value={1}>-16 LUFS | Podcast</option>
            <option value={2}>-23 LUFS | Broadcast</option>
          </select>
        </label>
        <label>
          {t("gpuEncoder")}
          <select
            value={gpu}
            title={t("gpuEncoderHint")}
            onChange={(e) => {
              setGpu(e.target.value);
              void run(api.setGpuEncoderPreference(e.target.value));
              setSavedTick(Date.now());
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
        <PathField
          label={t("outputFolder")}
          value={prefs.outputFolder}
          kind="folder"
          onChange={(value) => {
            update({ outputFolder: value });
          }}
        />
      </Group>
      <Group title={t("settingsModels")}>
        <PathField
          label={t("whisperModelPath")}
          value={prefs.whisperModelPath}
          kind="file"
          extensions={["bin"]}
          onChange={(value) => {
            update({ whisperModelPath: value });
          }}
        />
        <PathField
          label={t("reframeModelPath")}
          value={prefs.reframeModelPath}
          kind="file"
          extensions={["onnx"]}
          onChange={(value) => {
            update({ reframeModelPath: value });
          }}
        />
        <PathField
          label={t("backgroundRemovalModelPath")}
          value={prefs.backgroundRemovalModelPath}
          kind="file"
          extensions={["onnx"]}
          onChange={(value) => {
            update({ backgroundRemovalModelPath: value });
          }}
        />
        <PathField
          label={t("ttsModelPath")}
          value={prefs.ttsModelPath}
          kind="file"
          extensions={["onnx"]}
          onChange={(value) => {
            update({ ttsModelPath: value });
          }}
        />
        <PathField
          label={t("soundLibraryPath")}
          value={prefs.soundLibraryPath}
          kind="folder"
          onChange={(value) => {
            update({ soundLibraryPath: value });
          }}
        />
      </Group>
      <Group title={t("settingsPrivacy")}>
        <label className="check">
          <input
            type="checkbox"
            checked={prefs.telemetryEnabled}
            onChange={(e) => {
              return update({ telemetryEnabled: e.target.checked });
            }}
          />{" "}
          {t("telemetryEnabled")}
        </label>
        <label className="check">
          <input
            type="checkbox"
            checked={prefs.errorReportingConsent === "always_send"}
            onChange={(e) => {
              return setConsent(e.target.checked ? "always_send" : "disabled");
            }}
          />
          {t("errorReportingConsent")}
        </label>
        {queuedReports > 0 && (
          <label>
            {queuedReports} {t("queuedReports")}
            <button
              className="icon"
              onClick={() => {
                return void run(api.deleteQueuedReports()).then(() => {
                  return setQueuedReports(0);
                });
              }}
            >
              {t("deleteQueuedReports")}
            </button>
          </label>
        )}
      </Group>
      <Group title={t("settingsUpdates")}>
        <label>
          <button
            onClick={() => {
              return run(api.checkForUpdate());
            }}
          >
            {t("checkForUpdate")}
          </button>
          {updateStatus?.type === "upToDate" && (
            <span className="muted">{t("updateCheckUpToDate")}</span>
          )}
          {updateStatus?.type === "failed" && (
            <span className="muted">{t("updateCheckFailed")}</span>
          )}
          {updateStatus?.type === "newerVersionAvailable" && (
            <span className="muted">
              {t("updateAvailable")}: {updateStatus.version}{" "}
              {updateStatus.autoUpdateAvailable ? (
                <button
                  className="icon"
                  onClick={() => {
                    return run(api.installAvailableUpdate(updateStatus.version));
                  }}
                >
                  {t("updateInstall")}
                </button>
              ) : (
                <a
                  href={updateStatus.htmlUrl}
                  target="_blank"
                  rel="noreferrer"
                >
                  {t("updateOpenRelease")}
                </a>
              )}
            </span>
          )}
          {updateStatus?.type === "installed" && (
            <span className="muted">
              {t("updateInstalled")}{" "}
              <button
                className="icon"
                onClick={() => {
                  return run(api.restartAfterUpdate());
                }}
              >
                {t("updateRestart")}
              </button>
            </span>
          )}
          {updateStatus?.type === "installFailed" && (
            <span className="muted">{t("updateInstallFailed")}</span>
          )}
        </label>
      </Group>
      <p
        className="settings-saved"
        role="status"
      >
        {savedTick !== 0 ? t("settingsSaved") : ""}
      </p>
    </div>
  );
}
