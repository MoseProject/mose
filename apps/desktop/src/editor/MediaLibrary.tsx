import { useState } from "react";
import { formatTime, t } from "../i18n";
import { api, thumbnailUrl } from "../ipc";
import { assetDragStore, run } from "../store";
import type { MediaAsset, SessionView } from "../types";

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
