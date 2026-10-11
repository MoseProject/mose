import homeStyles from "../screens/home.module.css";
import { cx } from "../cx";
import baseStyles from "../styles/base.module.css";
import styles from "./chrome/chrome.module.css";
import { useState } from "react";
import { t } from "../i18n";
import { api } from "../ipc";
import { layerTemplatesStore, run, uiStore, useStore } from "../store";
import type { SessionView } from "../types";
import { Dialog } from "./chrome/Dialog";

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
      <button
        className={cx(styles, baseStyles, homeStyles, "icon")}
        onClick={() => {
          return setListOpen((v) => {
            return !v;
          });
        }}
      >
        {t("templates")}
      </button>
      <button
        className={cx(styles, baseStyles, "icon")}
        disabled={selected.length < 2}
        onClick={() => {
          return setSaving("");
        }}
      >
        {t("saveAsTemplate")}
      </button>

      {listOpen && (
        <Dialog
          title={t("templates")}
          onClose={() => {
            setListOpen(false);
          }}
        >
          {templates.length === 0 && (
            <p className={cx(styles, baseStyles, "muted")}>{t("noSavedTemplates")}</p>
          )}
          {templates.map((template, index) => {
            return (
              <div
                className={cx(styles, baseStyles, "modal-row")}
                key={index}
              >
                <span>{template.name}</span>
                <div>
                  <button
                    className={cx(styles, baseStyles, "icon")}
                    onClick={() => {
                      setApplying({
                        index,
                        assetIds: template.layers.map(() => {
                          return null;
                        }),
                      });
                      setListOpen(false);
                    }}
                  >
                    {t("applyTemplate")}
                  </button>
                  <button
                    className={cx(styles, baseStyles, "icon")}
                    onClick={() => {
                      return run(api.deleteLayerTemplate(index)).then((updated) => {
                        if (updated) {
                          layerTemplatesStore.set(updated);
                        }
                      });
                    }}
                  >
                    {t("deleteTemplate")}
                  </button>
                </div>
              </div>
            );
          })}
        </Dialog>
      )}

      {saving !== null && (
        <Dialog
          title={t("saveTemplateTitle")}
          onClose={() => {
            setSaving(null);
          }}
        >
          <input
            autoFocus
            value={saving}
            onChange={(e) => {
              return setSaving(e.target.value);
            }}
            onKeyDown={(e) => {
              return e.key === "Enter" && saving.trim() && confirmSave();
            }}
          />
          <div className={cx(styles, baseStyles, "modal-actions")}>
            <button
              className={cx(styles, baseStyles, "icon")}
              onClick={() => {
                return setSaving(null);
              }}
            >
              {t("cancel")}
            </button>
            <button
              className={cx(styles, baseStyles, "icon primary")}
              disabled={saving.trim().length === 0}
              onClick={confirmSave}
            >
              {t("saveTemplateConfirm")}
            </button>
          </div>
        </Dialog>
      )}

      {applying && (
        <Dialog
          title={t("applyTemplateTitle")}
          onClose={() => {
            setApplying(null);
          }}
        >
          {templates[applying.index]?.layers.map(([kind], layerIndex) => {
            const options = session.project.media_library.filter((a) => {
              return a.kind === kind;
            });
            return (
              <div
                className={cx(styles, baseStyles, "modal-row")}
                key={layerIndex}
              >
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
                  {options.map((asset) => {
                    return (
                      <option
                        key={asset.id}
                        value={asset.id}
                      >
                        {asset.file_name}
                      </option>
                    );
                  })}
                </select>
              </div>
            );
          })}
          <div className={cx(styles, baseStyles, "modal-actions")}>
            <button
              className={cx(styles, baseStyles, "icon")}
              onClick={() => {
                return setApplying(null);
              }}
            >
              {t("cancel")}
            </button>
            <button
              className={cx(styles, baseStyles, "icon primary")}
              disabled={applying.assetIds.every((id) => {
                return id === null;
              })}
              onClick={() => {
                void run(api.applyLayerTemplate(applying.index, applying.assetIds));
                setApplying(null);
              }}
            >
              {t("applyTemplateConfirm")}
            </button>
          </div>
        </Dialog>
      )}
    </>
  );

  function confirmSave() {
    if (!saving || saving.trim().length === 0) {
      return;
    }
    const name = saving.trim();
    setSaving(null);
    void run(api.saveLayerTemplate(name, selected)).then((updated) => {
      if (updated) {
        layerTemplatesStore.set(updated);
      }
    });
  }
}
