import { useState } from "react";
import { formatTime, t } from "../i18n";
import { Icon } from "./Icon";
import { ContextMenu } from "./ContextMenu";
import { Menu } from "./Menu";
import { api, onYoutubeDownload, thumbnailUrl } from "../ipc";
import { assetDragStore, run, transcribingAssetIdStore, useStore } from "../store";
import type { LibraryTrack, MediaAsset, SessionView } from "../types";
import { Dialog } from "./Dialog";

function YoutubeDownloadButton() {
  const [open, setOpen] = useState(false);
  const [url, setUrl] = useState("");
  const [format, setFormat] = useState<"mp4" | "mp3">("mp4");
  const [quality, setQuality] = useState("best");
  const [downloading, setDownloading] = useState(false);
  const [progress, setProgress] = useState(0);
  const [error, setError] = useState<string | null>(null);

  const openModal = () => {
    setOpen(true);
    setError(null);
    void run(api.isYtDlpAvailable()).then((available) => {
      if (available === false) {
        setError(t("youtubeDownloadToolMissing"));
      }
    });
  };

  const start = () => {
    if (!url.trim()) {
      return;
    }
    setDownloading(true);
    setProgress(0);
    setError(null);
    const unlisten = onYoutubeDownload((event) => {
      if (event.type === "progress") {
        setProgress(event.fraction);
      }
      if (event.type === "done") {
        setDownloading(false);
        setOpen(false);
        setUrl("");
        void unlisten.then((stop) => {
          return stop();
        });
      }
      if (event.type === "failed") {
        setDownloading(false);
        setError(event.message);
        void unlisten.then((stop) => {
          return stop();
        });
      }
    });
    // `run` swallows the rejection and resolves undefined, so success is marked explicitly.
    void run(
      api.downloadYoutube(url, format, quality).then(() => {
        return true;
      }),
    ).then((started) => {
      if (started) {
        return;
      }
      setDownloading(false);
      void unlisten.then((stop) => {
        return stop();
      });
    });
  };

  if (!open) {
    return (
      <button
        className="icon"
        onClick={openModal}
      >
        {t("youtubeDownload")}
      </button>
    );
  }

  return (
    <Dialog
      title={t("youtubeDownload")}
      onClose={() => {
        if (!downloading) {
          setOpen(false);
        }
      }}
    >
      {error && <p style={{ color: "var(--error, #e55)" }}>{error}</p>}
      <input
        autoFocus
        disabled={downloading}
        placeholder={t("youtubeDownloadUrlHint")}
        value={url}
        onChange={(e) => {
          return setUrl(e.target.value);
        }}
      />
      <div className="modal-row">
        <select
          disabled={downloading}
          value={format}
          onChange={(e) => {
            setFormat(e.target.value as "mp4" | "mp3");
            setQuality(e.target.value === "mp4" ? "best" : "192");
          }}
        >
          <option value="mp4">MP4</option>
          <option value="mp3">MP3</option>
        </select>
        <select
          disabled={downloading}
          value={quality}
          onChange={(e) => {
            return setQuality(e.target.value);
          }}
        >
          {format === "mp4"
            ? ["best", "1080", "720", "480", "360"].map((q) => {
                return (
                  <option
                    key={q}
                    value={q}
                  >
                    {q === "best" ? "Best" : `${q}p`}
                  </option>
                );
              })
            : ["320", "192", "128"].map((q) => {
                return (
                  <option
                    key={q}
                    value={q}
                  >
                    {q} kbps
                  </option>
                );
              })}
        </select>
      </div>
      {downloading && (
        <progress
          value={progress}
          max={1}
          style={{ width: "100%" }}
        />
      )}
      <div className="modal-actions">
        {downloading ? (
          <button
            className="icon"
            onClick={() => {
              return void run(api.cancelYoutubeDownload());
            }}
          >
            {t("youtubeDownloadCancel")}
          </button>
        ) : (
          <>
            <button
              className="icon"
              onClick={() => {
                return setOpen(false);
              }}
            >
              {t("cancel")}
            </button>
            <button
              className="icon primary"
              disabled={url.trim().length === 0}
              onClick={start}
            >
              {t("youtubeDownloadStart")}
            </button>
          </>
        )}
      </div>
    </Dialog>
  );
}

function SoundLibraryButton() {
  const [open, setOpen] = useState(false);
  const [tracks, setTracks] = useState<LibraryTrack[] | null>(null);

  const toggle = () => {
    if (!open) {
      void run(api.listSoundLibrary()).then((result) => {
        return setTracks(result ?? []);
      });
    }
    setOpen(!open);
  };

  return (
    <>
      <button
        className="icon"
        onClick={toggle}
      >
        {t("soundLibrary")}
      </button>
      {open && (
        <Dialog
          title={t("soundLibrary")}
          onClose={() => {
            setOpen(false);
          }}
        >
          {!tracks || tracks.length === 0 ? (
            <p className="muted">{t("soundLibraryEmpty")}</p>
          ) : (
            <div style={{ maxHeight: 320, overflowY: "auto" }}>
              {tracks.map((track) => {
                return (
                  <div
                    className="modal-row"
                    key={track.path}
                  >
                    <span>
                      {track.category === "music" ? "🎵" : "🔊"} {track.name} (
                      {formatTime(track.durationSecs)})
                    </span>
                    <button
                      className="icon"
                      onClick={() => {
                        void run(api.importMedia([track.path]));
                        setOpen(false);
                      }}
                    >
                      {t("importFiles")}
                    </button>
                  </div>
                );
              })}
            </div>
          )}
        </Dialog>
      )}
    </>
  );
}

function TtsButton() {
  const [text, setText] = useState<string | null>(null);
  if (text === null) {
    return (
      <button
        className="icon"
        onClick={() => {
          return setText("");
        }}
      >
        {t("ttsButton")}
      </button>
    );
  }
  return (
    <Dialog
      title={t("ttsModalTitle")}
      onClose={() => {
        setText(null);
      }}
    >
      <textarea
        autoFocus
        rows={4}
        style={{ width: "100%" }}
        placeholder={t("ttsPlaceholder")}
        value={text}
        onChange={(e) => {
          return setText(e.target.value);
        }}
      />
      <div className="modal-actions">
        <button
          className="icon"
          onClick={() => {
            return setText(null);
          }}
        >
          {t("cancel")}
        </button>
        <button
          className="icon primary"
          disabled={text.trim().length === 0}
          onClick={() => {
            void run(api.generateTts(text));
            setText(null);
          }}
        >
          {t("ttsGenerate")}
        </button>
      </div>
    </Dialog>
  );
}

/** Pixels the pointer must travel before a press on an asset becomes a drag. */
const DRAG_THRESHOLD_PX = 4;

type LibraryTab = "media" | "favorites" | "recent";
const TABS: [LibraryTab, "libraryTabMedia" | "libraryTabFavorites" | "libraryTabRecent"][] = [
  ["media", "libraryTabMedia"],
  ["favorites", "libraryTabFavorites"],
  ["recent", "libraryTabRecent"],
];

export function MediaLibrary({ session }: { session: SessionView }) {
  const [tab, setTab] = useState<LibraryTab>("media");
  const [query, setQuery] = useState("");
  const [view, setView] = useState<"grid" | "list">("grid");
  const [menu, setMenu] = useState<{ x: number; y: number; asset: MediaAsset } | null>(null);
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
      className="panel library"
      aria-label={t("mediaLibrary")}
    >
      <div
        className="panel-tabs"
        role="tablist"
      >
        {TABS.map(([id, label]) => {
          return (
            <button
              key={id}
              role="tab"
              aria-selected={tab === id}
              className={tab === id ? "active" : ""}
              onClick={() => {
                return setTab(id);
              }}
            >
              {t(label)}
            </button>
          );
        })}
        <button
          className="import-btn"
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
        >
          <TtsButton />
          <SoundLibraryButton />
          <YoutubeDownloadButton />
        </Menu>
      </div>
      <div className="search-row">
        <label className="search">
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
          className={`icon ${view === "grid" ? "active" : ""}`}
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
          className={`icon ${view === "list" ? "active" : ""}`}
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
      <div className="media-head">
        <span>{t("projectMedia")}</span>
        <span className="count">
          {assets.length} {t("items")}
        </span>
      </div>
      <div className={`asset-list ${view}`}>
        {assets.length === 0 ? (
          <div className="empty-state">
            {all.length === 0 ? t("libraryEmpty") : t("libraryNoMatches")}
          </div>
        ) : (
          assets.map((asset) => {
            return (
              <AssetRow
                key={asset.id}
                onContextMenu={(e) => {
                  e.preventDefault();
                  setMenu({ x: e.clientX, y: e.clientY, asset });
                }}
                asset={asset}
              />
            );
          })
        )}
      </div>
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
          ]}
          onClose={() => {
            setMenu(null);
          }}
        />
      )}
    </section>
  );
}

function AssetRow({
  asset,
  onContextMenu,
}: {
  asset: MediaAsset;
  onContextMenu: (e: React.MouseEvent) => void;
}) {
  const [thumbFailed, setThumbFailed] = useState(false);
  const transcribingAssetId = useStore(transcribingAssetIdStore);
  const transcribing = transcribingAssetId === asset.id;

  // Internal drag onto the timeline uses pointer events: with Tauri's OS file drop enabled,
  // WebView2 does not deliver HTML5 drag-and-drop between elements.
  const onPointerDown = (e: React.PointerEvent) => {
    if (e.button !== 0) {
      return;
    }
    const startX = e.clientX;
    const startY = e.clientY;
    let dragging = false;
    const move = (ev: PointerEvent) => {
      if (!dragging && Math.hypot(ev.clientX - startX, ev.clientY - startY) < DRAG_THRESHOLD_PX) {
        return;
      }
      dragging = true;
      assetDragStore.set({ assetId: asset.id, clientX: ev.clientX, clientY: ev.clientY });
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      // A macrotask, not a microtask: the timeline's own pointerup listener must still see
      // the drag when it runs after this one.
      setTimeout(() => {
        return assetDragStore.set(null);
      }, 0);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };

  const meta = [
    asset.resolution ? `${asset.resolution[0]}×${asset.resolution[1]}` : null,
    asset.fps ? `${asset.fps.toFixed(0)} fps` : null,
    asset.loudness ? `${asset.loudness.integrated_lufs.toFixed(1)} LUFS` : t("analyzing"),
  ]
    .filter(Boolean)
    .join(" · ");

  const actions = (
    <div className="asset-actions">
      {asset.has_audio && (
        <button
          className="icon"
          title={transcribing ? t("transcribing") : t("transcribe")}
          aria-label={transcribing ? t("transcribing") : t("transcribe")}
          disabled={transcribingAssetId !== null}
          onPointerDown={(e) => {
            return e.stopPropagation();
          }}
          onClick={() => {
            return run(api.transcribeAsset(asset.id));
          }}
        >
          {transcribing ? "…" : <Icon name="document" />}
        </button>
      )}
      <button
        className={`icon ${asset.favorited ? "active" : ""}`}
        title={t("favorite")}
        aria-label={t("favorite")}
        aria-pressed={asset.favorited}
        onPointerDown={(e) => {
          return e.stopPropagation();
        }}
        onClick={() => {
          return run(api.toggleFavorite(asset.id));
        }}
      >
        <Icon
          name="star"
          filled={asset.favorited}
        />
      </button>
    </div>
  );

  return (
    <div
      className={`asset ${asset.kind === "Audio" ? "audio" : ""}`}
      title={asset.source_path}
      tabIndex={0}
      onContextMenu={onContextMenu}
      onKeyDown={(e) => {
        if (e.key === "Enter" && e.target === e.currentTarget) {
          e.preventDefault();
          void run(api.addAssetToTimeline(asset.id));
        }
      }}
      onPointerDown={onPointerDown}
      onDoubleClick={() => {
        return run(api.addAssetToTimeline(asset.id));
      }}
    >
      {asset.kind === "Audio" ? (
        <div className="audio-card">
          <span className="audio-icon">
            <Icon
              name="music"
              size={20}
            />
          </span>
          <div className="audio-text">
            <div className="name">{asset.file_name}</div>
            <div className="meta">
              {formatTime(asset.duration_secs)} {meta && `· ${meta}`}
            </div>
          </div>
          {actions}
        </div>
      ) : (
        <>
          <div className="thumb-wrap">
            {!thumbFailed ? (
              <img
                className="thumb"
                src={thumbnailUrl(asset.id)}
                alt=""
                draggable={false}
                onError={() => {
                  return setThumbFailed(true);
                }}
              />
            ) : (
              <div className="thumb">
                <Icon
                  name="film"
                  size={22}
                />
              </div>
            )}
            <span className="duration mono">{formatTime(asset.duration_secs)}</span>
            {actions}
          </div>
          <div className="name">{asset.file_name}</div>
          <div className="meta">{meta}</div>
        </>
      )}
    </div>
  );
}
