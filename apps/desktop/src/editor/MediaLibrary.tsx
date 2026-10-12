import editorStyles from "../screens/editor.module.css";
import { cx } from "../cx";
import baseStyles from "../styles/base.module.css";
import styles from "./library/library.module.css";
import { Suspense, useCallback, useState } from "react";
import { lazyNamed } from "../lazy";
import { t } from "../i18n";
import { Icon } from "./chrome/Icon";
import { ContextMenu } from "./chrome/ContextMenu";
import { Menu } from "./chrome/Menu";
import { api } from "../ipc";
import { askConfirm, pushToast } from "../notifications";
import { run } from "../store";
import type { MediaAsset, SessionView } from "../types";
import { AssetRow } from "./library/AssetRow";

// Not on screen until the "more sources" menu is opened, or the Effects tab chosen.
const YoutubeDownloadButton = lazyNamed(() => {
  return import("./library/YoutubeDownloadButton");
}, "YoutubeDownloadButton");
const SoundLibraryButton = lazyNamed(() => {
  return import("./library/SoundLibraryButton");
}, "SoundLibraryButton");
const TtsButton = lazyNamed(() => {
  return import("./library/TtsButton");
}, "TtsButton");
const EffectsBrowser = lazyNamed(() => {
  return import("./library/EffectsBrowser");
}, "EffectsBrowser");

/** Whether any clip in any sequence plays `assetId`. */
function assetInUse(session: SessionView, assetId: number): boolean {
  return session.project.sequences.some((sequence) => {
    return sequence.timeline.tracks.some((track) => {
      return track.clips.some((clip) => {
        return clip.asset_id === assetId;
      });
    });
  });
}

/** Removes media from the library after a confirmation; media still on a timeline is refused. */
async function removeAsset(session: SessionView, asset: MediaAsset) {
  if (assetInUse(session, asset.id)) {
    pushToast(t("assetInUse"), "error");
    return;
  }
  const confirmed = await askConfirm({
    title: t("removeFromLibraryTitle"),
    message: asset.file_name + ". " + t("removeFromLibraryMessage"),
    confirmLabel: t("removeFromLibrary"),
    cancelLabel: t("cancel"),
  });
  if (confirmed) {
    await run(api.removeAsset(asset.id));
  }
}

/** Pixels the pointer must travel before a press on an asset becomes a drag. */
type LibraryTab = "media" | "favorites" | "recent" | "effects";
const TABS: [
  LibraryTab,
  "libraryTabMedia" | "libraryTabFavorites" | "libraryTabRecent" | "libraryTabEffects",
][] = [
  ["media", "libraryTabMedia"],
  ["favorites", "libraryTabFavorites"],
  ["recent", "libraryTabRecent"],
  ["effects", "libraryTabEffects"],
];

export function MediaLibrary({ session }: { session: SessionView }) {
  const [tab, setTab] = useState<LibraryTab>("media");
  const [query, setQuery] = useState("");
  const [view, setView] = useState<"grid" | "list">("grid");
  const [menu, setMenu] = useState<{ x: number; y: number; asset: MediaAsset } | null>(null);
  // Stable, so the memoized rows are not rebuilt by every zoom or scroll of the timeline.
  const openAssetMenu = useCallback((asset: MediaAsset, e: React.MouseEvent) => {
    e.preventDefault();
    setMenu({ x: e.clientX, y: e.clientY, asset });
  }, []);
  const all = session.project.media_library;

  const inTab =
    tab === "favorites"
      ? all.filter((asset) => {
          return asset.favorited;
        })
      : tab === "recent"
        ? session.project.recent_asset_ids
            .map((id) => {
              return all.find((asset) => {
                return asset.id === id;
              });
            })
            .filter((asset): asset is MediaAsset => {
              return asset !== undefined;
            })
        : all;
  const needle = query.trim().toLowerCase();
  const assets = needle
    ? inTab.filter((asset) => {
        return asset.file_name.toLowerCase().includes(needle);
      })
    : inTab;

  return (
    <section
      className={cx(styles, baseStyles, editorStyles, "panel library")}
      aria-label={t("mediaLibrary")}
    >
      <div
        className={cx(styles, baseStyles, "panel-tabs")}
        role="tablist"
      >
        {TABS.map(([id, label]) => {
          return (
            <button
              key={id}
              role="tab"
              aria-selected={tab === id}
              className={cx(styles, baseStyles, tab === id ? "active" : "")}
              onClick={() => {
                return setTab(id);
              }}
            >
              {t(label)}
            </button>
          );
        })}
      </div>
      {tab === "effects" ? (
        <Suspense fallback={null}>
          <EffectsBrowser session={session} />
        </Suspense>
      ) : (
        <>
          <div className={cx(styles, baseStyles, "search-row")}>
            <label className={cx(styles, baseStyles, "search")}>
              <Icon
                name="search"
                size={14}
              />
              <input
                type="search"
                value={query}
                placeholder={t("searchMedia")}
                aria-label={t("searchMedia")}
                onChange={(e) => {
                  return setQuery(e.target.value);
                }}
              />
            </label>
            <button
              className={cx(styles, baseStyles, `icon ${view === "grid" ? "active" : ""}`)}
              aria-label={t("viewGrid")}
              title={t("viewGrid")}
              aria-pressed={view === "grid"}
              onClick={() => {
                return setView("grid");
              }}
            >
              <Icon
                name="grid"
                size={16}
              />
            </button>
            <button
              className={cx(styles, baseStyles, `icon ${view === "list" ? "active" : ""}`)}
              aria-label={t("viewList")}
              title={t("viewList")}
              aria-pressed={view === "list"}
              onClick={() => {
                return setView("list");
              }}
            >
              <Icon
                name="list"
                size={16}
              />
            </button>
          </div>
          <div className={cx(styles, baseStyles, "media-head")}>
            <span>{t("projectMedia")}</span>
            <span className={cx(styles, baseStyles, "count")}>
              {assets.length} {t("items")}
            </span>
            <button
              className={cx(styles, baseStyles, "import-btn")}
              onClick={() => {
                return run(api.importMediaDialog());
              }}
            >
              <Icon
                name="plus"
                size={14}
              />{" "}
              {t("importFiles")}
            </button>
            <Menu
              label={
                <Icon
                  name="more"
                  size={16}
                />
              }
              ariaLabel={t("moreSources")}
              deferred
            >
              <Suspense fallback={null}>
                <TtsButton />
                <SoundLibraryButton />
                <YoutubeDownloadButton />
              </Suspense>
            </Menu>
          </div>
          <div className={cx(styles, baseStyles, `asset-list ${view}`)}>
            {assets.length === 0 ? (
              <div className={cx(styles, baseStyles, "empty-state")}>
                {all.length === 0 ? t("libraryEmpty") : t("libraryNoMatches")}
              </div>
            ) : (
              assets.map((asset) => {
                return (
                  <AssetRow
                    key={asset.id}
                    onContextMenu={openAssetMenu}
                    asset={asset}
                  />
                );
              })
            )}
          </div>
        </>
      )}
      {menu && (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          items={[
            {
              label: t("addToTimeline"),
              onSelect: () => {
                void run(api.addAssetToTimeline(menu.asset.id));
              },
            },
            {
              label: t("favorite"),
              onSelect: () => {
                void run(api.toggleFavorite(menu.asset.id));
              },
            },
            {
              label: t("transcribe"),
              disabled: !menu.asset.has_audio,
              onSelect: () => {
                void run(api.transcribeAsset(menu.asset.id));
              },
            },
            {
              label: t("removeFromLibrary"),
              separator: true,
              onSelect: () => {
                void removeAsset(session, menu.asset);
              },
            },
          ]}
          onClose={() => {
            setMenu(null);
          }}
        />
      )}
    </section>
  );
}
