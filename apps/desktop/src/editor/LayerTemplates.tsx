import { useState } from "react";
import { t } from "../i18n";
import { api } from "../ipc";
import { layerTemplatesStore, run, uiStore, useStore } from "../store";
import type { SessionView } from "../types";

/** Layer-group template list/save/apply popups -- egui's narrow `egui::Modal` exception
 *  (`spec/architecture/tauri-migration.md`'s parity notes), not a general-purpose dialog system. */
export function LayerTemplates({ session }: { session: SessionView }) {
  const templates = useStore(layerTemplatesStore);
  const ui = useStore(uiStore);
  const [listOpen, setListOpen] = useState(false);
  const [saving, setSaving] = useState<string | null>(null);
  const [applying, setApplying] = useState<{ index: number; assetIds: (number | null)[] } | null>(
    null,
  );

  const selected = ui.selectedClipIds;

  return (
    <>
      <button className="icon" onClick={() => setListOpen((v) => !v)}>
        {t("templates")}
      </button>
      <button
        className="icon"
        disabled={selected.length < 2}
        onClick={() => setSaving("")}
      >
        {t("saveAsTemplate")}
      </button>

      {listOpen && (
        <div className="modal-overlay" onClick={() => setListOpen(false)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <h3>{t("templates")}</h3>
            {templates.length === 0 && <p className="muted">{t("noSavedTemplates")}</p>}
            {templates.map((template, index) => (
              <div className="modal-row" key={index}>
                <span>{template.name}</span>
                <div>
                  <button
                    className="icon"
                    onClick={() => {
                      setApplying({ index, assetIds: template.layers.map(() => null) });
                      setListOpen(false);
                    }}
                  >
                    {t("applyTemplate")}
                  </button>
                  <button
                    className="icon"
                    onClick={() =>
                      run(api.deleteLayerTemplate(index)).then((updated) => {
                        if (updated) layerTemplatesStore.set(updated);
                      })
                    }
                  >
                    {t("deleteTemplate")}
                  </button>
                </div>
              </div>
            ))}
          </div>
        </div>
      )}

      {saving !== null && (
        <div className="modal-overlay" onClick={() => setSaving(null)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <h3>{t("saveTemplateTitle")}</h3>
            <input
              autoFocus
              value={saving}
              onChange={(e) => setSaving(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && saving.trim() && confirmSave()}
            />
            <div className="modal-actions">
              <button className="icon" onClick={() => setSaving(null)}>
                {t("cancel")}
              </button>
              <button
                className="icon primary"
                disabled={saving.trim().length === 0}
                onClick={confirmSave}
              >
                {t("saveTemplateConfirm")}
              </button>
            </div>
          </div>
        </div>
      )}

      {applying && (
        <div className="modal-overlay" onClick={() => setApplying(null)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <h3>{t("applyTemplateTitle")}</h3>
            {templates[applying.index]?.layers.map(([kind], layerIndex) => {
              const options = session.project.media_library.filter((a) => a.kind === kind);
              return (
                <div className="modal-row" key={layerIndex}>
                  <span>
                    {t("applyTemplateLayerLabel")} {layerIndex + 1} ({kind})
                  </span>
                  <select
                    value={applying.assetIds[layerIndex] ?? ""}
                    onChange={(e) => {
                      const assetIds = [...applying.assetIds];
                      assetIds[layerIndex] = e.target.value ? Number(e.target.value) : null;
                      setApplying({ ...applying, assetIds });
                    }}
                  >
                    <option value="">{t("applyTemplatePickAsset")}</option>
                    {options.map((asset) => (
                      <option key={asset.id} value={asset.id}>
                        {asset.file_name}
                      </option>
                    ))}
                  </select>
                </div>
              );
            })}
            <div className="modal-actions">
              <button className="icon" onClick={() => setApplying(null)}>
                {t("cancel")}
              </button>
              <button
                className="icon primary"
                disabled={applying.assetIds.every((id) => id === null)}
                onClick={() => {
                  void run(api.applyLayerTemplate(applying.index, applying.assetIds));
                  setApplying(null);
                }}
              >
                {t("applyTemplateConfirm")}
              </button>
            </div>
          </div>
        </div>
      )}
    </>
  );

  function confirmSave() {
    if (!saving || saving.trim().length === 0) return;
    const name = saving.trim();
    setSaving(null);
    void run(api.saveLayerTemplate(name, selected)).then((updated) => {
      if (updated) layerTemplatesStore.set(updated);
    });
  }
}
