import { cx } from "../../cx";
import baseStyles from "../../styles/base.module.css";
import styles from "./library.module.css";
import { memo, useState } from "react";
import { formatTime, t } from "../../i18n";
import { Icon } from "../chrome/Icon";
import { api, thumbnailUrl } from "../../ipc";
import { assetDragStore, run, transcribingAssetIdStore, useStore } from "../../store";
import type { MediaAsset } from "../../types";
import { trackPointerDrag } from "./pointerDrag";

/** Memoized: the library re-renders with every zoom and scroll of the timeline. */
export const AssetRow = memo(function AssetRow({
  asset,
  onContextMenu,
}: {
  asset: MediaAsset;
  onContextMenu: (asset: MediaAsset, e: React.MouseEvent) => void;
}) {
  const [thumbFailed, setThumbFailed] = useState(false);
  const transcribingAssetId = useStore(transcribingAssetIdStore);
  const transcribing = transcribingAssetId === asset.id;

  const onPointerDown = (e: React.PointerEvent) => {
    if (e.button !== 0) {
      return;
    }
    trackPointerDrag(
      e,
      (ev) => {
        assetDragStore.set({ assetId: asset.id, clientX: ev.clientX, clientY: ev.clientY });
      },
      () => {
        assetDragStore.set(null);
      },
    );
  };

  const meta = [
    asset.resolution ? `${asset.resolution[0]}×${asset.resolution[1]}` : null,
    asset.fps ? `${asset.fps.toFixed(0)} fps` : null,
    asset.loudness ? `${asset.loudness.integrated_lufs.toFixed(1)} LUFS` : t("analyzing"),
  ]
    .filter(Boolean)
    .join(" · ");

  const actions = (
    <div className={cx(styles, baseStyles, "asset-actions")}>
      {asset.has_audio && (
        <button
          className={cx(styles, baseStyles, "icon")}
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
        className={cx(styles, baseStyles, `icon ${asset.favorited ? "active" : ""}`)}
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
      className={cx(styles, baseStyles, `asset ${asset.kind === "Audio" ? "audio" : ""}`)}
      title={asset.source_path}
      tabIndex={0}
      onContextMenu={(e) => {
        onContextMenu(asset, e);
      }}
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
        <div className={cx(styles, baseStyles, "audio-card")}>
          <span className={cx(styles, baseStyles, "audio-icon")}>
            <Icon
              name="music"
              size={20}
            />
          </span>
          <div className={cx(styles, baseStyles, "audio-text")}>
            <div className={cx(styles, baseStyles, "name")}>{asset.file_name}</div>
            <div className={cx(styles, baseStyles, "meta")}>
              {formatTime(asset.duration_secs)} {meta && `· ${meta}`}
            </div>
          </div>
          {actions}
        </div>
      ) : (
        <>
          <div className={cx(styles, baseStyles, "thumb-wrap")}>
            {!thumbFailed ? (
              <img
                className={cx(styles, baseStyles, "thumb")}
                src={thumbnailUrl(asset.id)}
                alt=""
                // Fetched when the card nears the view, not for every media file at once.
                loading="lazy"
                decoding="async"
                draggable={false}
                onError={() => {
                  return setThumbFailed(true);
                }}
              />
            ) : (
              <div className={cx(styles, baseStyles, "thumb")}>
                <Icon
                  name="film"
                  size={22}
                />
              </div>
            )}
            <span className={cx(styles, baseStyles, "duration mono")}>
              {formatTime(asset.duration_secs)}
            </span>
            {actions}
          </div>
          <div className={cx(styles, baseStyles, "name")}>{asset.file_name}</div>
          <div className={cx(styles, baseStyles, "meta")}>{meta}</div>
        </>
      )}
    </div>
  );
});
