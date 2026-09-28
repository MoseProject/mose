import { useEffect, useState } from "react";
import { localeFromPrefs, setLocale, t } from "../i18n";
import { api } from "../ipc";
import { run } from "../store";
import type { PrefsView } from "../types";

export function Prefs() {
  const [prefs, setPrefs] = useState<PrefsView | null>(null);

  useEffect(() => {
    void run(api.getPrefs()).then((value) => value && setPrefs(value));
  }, []);

  const update = (patch: Partial<PrefsView>) => {
    if (!prefs) return;
    const next = { ...prefs, ...patch };
    setPrefs(next);
    if (patch.locale) setLocale(localeFromPrefs(patch.locale));
    void run(api.setPrefs(next));
  };

  if (!prefs) return null;
  return <main className="prefs">
    <h1>{t("preferences")}</h1>
    <label>{t("language")}<select value={prefs.locale} onChange={(e) => update({ locale: e.target.value })}><option value="pt_br">Português</option><option value="en">English</option></select></label>
    <label>{t("defaultLufs")}<select value={prefs.lufsProfile} onChange={(e) => update({ lufsProfile: Number(e.target.value) })}><option value={0}>-14 LUFS | YouTube</option><option value={1}>-16 LUFS | Podcast</option><option value={2}>-23 LUFS | Broadcast</option></select></label>
    <label><input type="checkbox" checked={prefs.previewHardwareDecode} onChange={(e) => update({ previewHardwareDecode: e.target.checked })} /> {t("hardwareDecode")}</label>
    <label>{t("outputFolder")}<input value={prefs.outputFolder} onChange={(e) => update({ outputFolder: e.target.value })} /></label>
    <label>{t("autosaveInterval")}<input type="number" min={1} max={60} value={prefs.autosaveMinutes} onChange={(e) => update({ autosaveMinutes: Number(e.target.value) || 1 })} /> {t("minutes")}</label>
  </main>;
}
