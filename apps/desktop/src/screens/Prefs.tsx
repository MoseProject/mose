import { useEffect, useState } from "react";
import { localeFromPrefs, setLocale, t } from "../i18n";
import { api, onUpdateCheck } from "../ipc";
import { run } from "../store";
import type { ErrorReportingConsent, PrefsView, UpdateCheckEvent } from "../types";

export function Prefs() {
  const [prefs, setPrefs] = useState<PrefsView | null>(null);
  const [queuedReports, setQueuedReports] = useState(0);
  const [updateStatus, setUpdateStatus] = useState<UpdateCheckEvent | null>(null);

  useEffect(() => {
    void run(api.getPrefs()).then((value) => value && setPrefs(value));
    void run(api.getQueuedReportCount()).then((count) => setQueuedReports(count ?? 0));
    const unlisten = onUpdateCheck((event) => setUpdateStatus(event));
    return () => {
      void unlisten.then((stop) => stop());
    };
  }, []);

  const update = (patch: Partial<PrefsView>) => {
    if (!prefs) return;
    const next = { ...prefs, ...patch };
    setPrefs(next);
    if (patch.locale) setLocale(localeFromPrefs(patch.locale));
    void run(api.setPrefs(next));
  };

  const setConsent = (consent: ErrorReportingConsent) => {
    if (!prefs) return;
    setPrefs({ ...prefs, errorReportingConsent: consent });
    void run(api.setErrorReportingConsent(consent));
  };

  if (!prefs) return null;
  return <main className="prefs">
    <h1>{t("preferences")}</h1>
    <label>{t("language")}<select value={prefs.locale} onChange={(e) => update({ locale: e.target.value })}><option value="pt_br">Português</option><option value="en">English</option></select></label>
    <label>{t("defaultLufs")}<select value={prefs.lufsProfile} onChange={(e) => update({ lufsProfile: Number(e.target.value) })}><option value={0}>-14 LUFS | YouTube</option><option value={1}>-16 LUFS | Podcast</option><option value={2}>-23 LUFS | Broadcast</option></select></label>
    <label><input type="checkbox" checked={prefs.previewHardwareDecode} onChange={(e) => update({ previewHardwareDecode: e.target.checked })} /> {t("hardwareDecode")}</label>
    <label>{t("outputFolder")}<input value={prefs.outputFolder} onChange={(e) => update({ outputFolder: e.target.value })} /></label>
    <label>{t("autosaveInterval")}<input type="number" min={1} max={60} value={prefs.autosaveMinutes} onChange={(e) => update({ autosaveMinutes: Number(e.target.value) || 1 })} /> {t("minutes")}</label>
    <label>{t("whisperModelPath")}<input value={prefs.whisperModelPath} onChange={(e) => update({ whisperModelPath: e.target.value })} /></label>
    <label>{t("reframeModelPath")}<input value={prefs.reframeModelPath} onChange={(e) => update({ reframeModelPath: e.target.value })} /></label>
    <label>{t("backgroundRemovalModelPath")}<input value={prefs.backgroundRemovalModelPath} onChange={(e) => update({ backgroundRemovalModelPath: e.target.value })} /></label>
    <label>{t("ttsModelPath")}<input value={prefs.ttsModelPath} onChange={(e) => update({ ttsModelPath: e.target.value })} /></label>
    <label>{t("soundLibraryPath")}<input value={prefs.soundLibraryPath} onChange={(e) => update({ soundLibraryPath: e.target.value })} /></label>
    <label><input type="checkbox" checked={prefs.telemetryEnabled} onChange={(e) => update({ telemetryEnabled: e.target.checked })} /> {t("telemetryEnabled")}</label>
    <label>
      <input
        type="checkbox"
        checked={prefs.errorReportingConsent === "always_send"}
        onChange={(e) => setConsent(e.target.checked ? "always_send" : "disabled")}
      />
      {t("errorReportingConsent")}
    </label>
    {queuedReports > 0 && (
      <label>
        {queuedReports} {t("queuedReports")}
        <button
          className="icon"
          onClick={() =>
            void run(api.deleteQueuedReports()).then(() => setQueuedReports(0))
          }
        >
          {t("deleteQueuedReports")}
        </button>
      </label>
    )}
    <label>
      <button className="icon" onClick={() => run(api.checkForUpdate())}>
        {t("checkForUpdate")}
      </button>
      {updateStatus?.type === "upToDate" && <span className="muted">{t("updateCheckUpToDate")}</span>}
      {updateStatus?.type === "failed" && <span className="muted">{t("updateCheckFailed")}</span>}
      {updateStatus?.type === "newerVersionAvailable" && (
        <span className="muted">
          {t("updateAvailable")}: {updateStatus.version}{" "}
          {updateStatus.autoUpdateAvailable ? (
            <button className="icon" onClick={() => run(api.installAvailableUpdate(updateStatus.version))}>
              {t("updateInstall")}
            </button>
          ) : (
            <a href={updateStatus.htmlUrl} target="_blank" rel="noreferrer">
              {t("updateOpenRelease")}
            </a>
          )}
        </span>
      )}
      {updateStatus?.type === "installed" && (
        <span className="muted">
          {t("updateInstalled")}{" "}
          <button className="icon" onClick={() => run(api.restartAfterUpdate())}>
            {t("updateRestart")}
          </button>
        </span>
      )}
      {updateStatus?.type === "installFailed" && <span className="muted">{t("updateInstallFailed")}</span>}
    </label>
  </main>;
}
