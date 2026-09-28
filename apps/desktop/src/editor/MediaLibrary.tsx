import { useState } from "react";
import { formatTime, t } from "../i18n";
import { api, thumbnailUrl } from "../ipc";
import { assetDragStore, run, transcribingAssetIdStore, useStore } from "../store";
import type { LibraryTrack, MediaAsset, SessionView } from "../types";

function SoundLibraryButton() {
  const [open, setOpen] = useState(false);
  const [tracks, setTracks] = useState<LibraryTrack[] | null>(null);

  const toggle = () => {
    if (!open) void run(api.listSoundLibrary()).then((result) => setTracks(result ?? []));
    setOpen(!open);
  };

  return (
    <>
      <button className="icon" onClick={toggle}>
        {t("soundLibrary")}
      </button>
      {open && (
        <div className="modal-overlay" onClick={() => setOpen(false)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <h3>{t("soundLibrary")}</h3>
            {!tracks || tracks.length === 0 ? (
              <p className="muted">{t("soundLibraryEmpty")}</p>
            ) : (
              <div style={{ maxHeight: 320, overflowY: "auto" }}>
                {tracks.map((track) => (
                  <div className="modal-row" key={track.path}>
                    <span>
                      {track.category === "music" ? "🎵" : "🔊"} {track.name} ({formatTime(track.durationSecs)})
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
                ))}
              </div>
            )}
          </div>
        </div>
      )}
    </>
  );
}

function TtsButton() {
  const [text, setText] = useState<string | null>(null);
  if (text === null) {
    return (
      <button className="icon" onClick={() => setText("")}>
        {t("ttsButton")}
      </button>
    );
  }
  return (
    <div className="modal-overlay" onClick={() => setText(null)}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <h3>{t("ttsModalTitle")}</h3>
        <textarea
          autoFocus
          rows={4}
          style={{ width: "100%" }}
          placeholder={t("ttsPlaceholder")}
          value={text}
          onChange={(e) => setText(e.target.value)}
        />
        <div className="modal-actions">
          <button className="icon" onClick={() => setText(null)}>
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
      </div>
    </div>
  );
}

/** Pixels the pointer must travel before a press on an asset becomes a drag. */
const DRAG_THRESHOLD_PX = 4;

export function MediaLibrary({ session }: { session: SessionView }) {
  const assets = session.project.media_library;

  return (
    <section className="panel library">
      <div className="panel-title">
        <span>{t("mediaLibrary")}</span>
        <button className="icon" onClick={() => run(api.importMediaDialog())}>
          {t("importFiles")}
        </button>
        <TtsButton />
        <SoundLibraryButton />
      </div>
      <div className="asset-list">
        {assets.length === 0 ? (
          <div className="empty-state">{t("libraryEmpty")}</div>
        ) : (
          assets.map((asset) => <AssetRow key={asset.id} asset={asset} />)
        )}
      </div>
    </section>
  );
}

function AssetRow({ asset }: { asset: MediaAsset }) {
  const [thumbFailed, setThumbFailed] = useState(false);
  const transcribingAssetId = useStore(transcribingAssetIdStore);
  const transcribing = transcribingAssetId === asset.id;

  // Internal drag onto the timeline uses pointer events: with Tauri's OS file drop enabled,
  // WebView2 does not deliver HTML5 drag-and-drop between elements.
  const onPointerDown = (e: React.PointerEvent) => {
    if (e.button !== 0) return;
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
      setTimeout(() => assetDragStore.set(null), 0);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };

  const meta = [
    formatTime(asset.duration_secs),
    asset.resolution ? `${asset.resolution[0]}×${asset.resolution[1]}` : null,
    asset.fps ? `${asset.fps.toFixed(0)} fps` : null,
    asset.loudness ? `${asset.loudness.integrated_lufs.toFixed(1)} LUFS` : t("analyzing"),
  ]
    .filter(Boolean)
    .join(" · ");

  return (
    <div
      className="asset"
      title={asset.source_path}
      onPointerDown={onPointerDown}
      onDoubleClick={() => run(api.addAssetToTimeline(asset.id))}
    >
      {asset.kind === "Video" && !thumbFailed ? (
        <img
          className="thumb"
          src={thumbnailUrl(asset.id)}
          alt=""
          draggable={false}
          onError={() => setThumbFailed(true)}
        />
      ) : (
        <div className="thumb">{asset.kind === "Audio" ? "♪" : "▶"}</div>
      )}
      <div>
        <div className="name">{asset.file_name}</div>
        <div className="meta">{meta}</div>
      </div>
      {asset.has_audio && (
        <button
          className="icon"
          title={transcribing ? t("transcribing") : t("transcribe")}
          disabled={transcribingAssetId !== null}
          onPointerDown={(e) => e.stopPropagation()}
          onClick={() => run(api.transcribeAsset(asset.id))}
        >
          {transcribing ? "…" : "📝"}
        </button>
      )}
      <button
        className={`icon ${asset.favorited ? "active" : ""}`}
        title={t("favorite")}
        onPointerDown={(e) => e.stopPropagation()}
        onClick={() => run(api.toggleFavorite(asset.id))}
      >
        {asset.favorited ? "★" : "☆"}
      </button>
    </div>
  );
}
