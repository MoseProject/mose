import { cx } from "../../cx";
import baseStyles from "../../styles/base.module.css";
import styles from "./library.module.css";
import { useState } from "react";
import { t } from "../../i18n";
import { api } from "../../ipc";
import { run, uiStore, useStore } from "../../store";
import { activeTimeline, BLEND_MODES, type ClipPatch, type SessionView } from "../../types";
import { Icon } from "../chrome/Icon";

interface Effect {
  id: string;
  label: string;
  patch: ClipPatch;
}
interface Folder {
  id: string;
  label: string;
  effects: Effect[];
}

/** The effects a clip can take, as folders. Each one is a patch the inspector's own controls
 *  would send, so applying here and in the inspector end in the same command. */
function catalog(): Folder[] {
  return [
    {
      id: "transitions",
      label: t("effectsTransitions"),
      effects: [
        { id: "Fade", label: t("transitionFade"), patch: { transitionIn: "Fade" } },
        { id: "HardCut", label: t("transitionHardCut"), patch: { transitionIn: "HardCut" } },
        { id: "Slide", label: t("transitionSlide"), patch: { transitionIn: "Slide" } },
        { id: "Zoom", label: t("transitionZoom"), patch: { transitionIn: "Zoom" } },
        { id: "Dissolve", label: t("transitionDissolve"), patch: { transitionIn: "Dissolve" } },
      ],
    },
    {
      id: "masks",
      label: t("effectsMasks"),
      effects: [
        { id: "Circle", label: t("maskCircle"), patch: { maskShape: "Circle" } },
        { id: "RoundedRect", label: t("maskRoundedRect"), patch: { maskShape: "RoundedRect" } },
      ],
    },
    {
      id: "blend",
      label: t("effectsBlendModes"),
      effects: BLEND_MODES.map((mode) => {
        return { id: mode, label: mode, patch: { blendMode: mode } };
      }),
    },
  ];
}

/** Effects of `catalog()` whose name, or whose folder's name, contains `query`. */
export function filterCatalog(folders: Folder[], query: string): Folder[] {
  const needle = query.trim().toLowerCase();
  if (!needle) {
    return folders;
  }
  return folders
    .map((folder) => {
      const folderMatches = folder.label.toLowerCase().includes(needle);
      return {
        ...folder,
        effects: folderMatches
          ? folder.effects
          : folder.effects.filter((effect) => {
              return effect.label.toLowerCase().includes(needle);
            }),
      };
    })
    .filter((folder) => {
      return folder.effects.length > 0;
    });
}

/** A searchable tree of effects that applies the picked one to the selected video clips. */
export function EffectsBrowser({ session }: { session: SessionView }) {
  const [query, setQuery] = useState("");
  const { selectedClipIds } = useStore(uiStore);
  const videoIds = activeTimeline(session.project)
    .tracks.flatMap((track) => {
      return track.clips;
    })
    .filter((clip) => {
      return selectedClipIds.includes(clip.id);
    })
    .map((clip) => {
      return clip.id;
    });
  const folders = filterCatalog(catalog(), query);
  const apply = async (patch: ClipPatch) => {
    for (const id of videoIds) {
      await run(api.updateClip(id, patch));
    }
  };

  return (
    <div
      className={cx(styles, baseStyles, "effects-browser")}
      data-testid="effects-browser"
    >
      <div className={cx(styles, baseStyles, "search-row")}>
        <label className={cx(styles, baseStyles, "search")}>
          <Icon
            name="search"
            size={14}
          />
          <input
            type="search"
            value={query}
            placeholder={t("searchEffects")}
            aria-label={t("searchEffects")}
            onChange={(e) => {
              return setQuery(e.target.value);
            }}
          />
        </label>
      </div>
      {videoIds.length === 0 && (
        <p className={cx(styles, baseStyles, "effects-hint")}>{t("effectsNeedClip")}</p>
      )}
      <div className={cx(styles, baseStyles, "effects-tree")}>
        {folders.length === 0 ? (
          <div className={cx(styles, baseStyles, "empty-state")}>{t("effectsNoMatches")}</div>
        ) : (
          folders.map((folder) => {
            return (
              <details
                key={folder.id}
                open={query.trim() !== "" || folder.id === "transitions"}
                className={cx(styles, baseStyles, "effects-folder")}
              >
                <summary>{folder.label}</summary>
                {folder.effects.map((effect) => {
                  return (
                    <button
                      key={effect.id}
                      className={cx(styles, baseStyles, "effects-item")}
                      disabled={videoIds.length === 0}
                      data-testid={`effect-${folder.id}-${effect.id}`}
                      onClick={() => {
                        return apply(effect.patch);
                      }}
                    >
                      {effect.label}
                    </button>
                  );
                })}
              </details>
            );
          })
        )}
      </div>
    </div>
  );
}
