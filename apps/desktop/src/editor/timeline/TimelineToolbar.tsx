import editorStyles from "../../screens/editor.module.css";
import { cx } from "../../cx";
import baseStyles from "../../styles/base.module.css";
import styles from "./timeline.module.css";
import { t } from "../../i18n";
import { addShapeOverlay, addTextOverlay, seekToMarker } from "../../editing";
import { api } from "../../ipc";
import {
  COMMON_FRAME_RATES,
  FREE_KEY,
  frameRateFromKey,
  frameRateKey,
  frameRateText,
} from "../../frameRate";
import { Icon } from "../chrome/Icon";
import { SequenceTabs } from "../SequenceTabs";
import { Menu } from "../chrome/Menu";
import { Suspense } from "react";
import { lazyNamed } from "../../lazy";
import { patchUi, run } from "../../store";
import { type SessionView } from "../../types";
import type { Marker } from "../../types";

// Menu entries that open their own dialog: fetched when their menu is first opened.
const LayerTemplates = lazyNamed(() => {
  return import("../LayerTemplates");
}, "LayerTemplates");
const SilenceReviewPanel = lazyNamed(() => {
  return import("../SilenceReviewPanel");
}, "SilenceReviewPanel");
const TranscriptPanel = lazyNamed(() => {
  return import("../TranscriptPanel");
}, "TranscriptPanel");

/** The timeline's header: sequence tabs, edit and add menus, marker and clip tools, and zoom. */
export function TimelineToolbar({
  session,
  selected,
  markers,
  zoomBy,
  zoomToFit,
}: {
  session: SessionView;
  selected: number[];
  markers: Marker[];
  zoomBy: (factor: number) => void;
  zoomToFit: () => void;
}) {
  return (
    <div className={cx(styles, baseStyles, editorStyles, "timeline-toolbar")}>
      <span
        className={cx(styles, baseStyles, "panel-title")}
        style={{ border: 0, padding: "0 8px 0 0" }}
      >
        {t("timeline")}
      </span>
      <SequenceTabs session={session} />
      <button
        className={cx(styles, baseStyles, "icon")}
        disabled={selected.length === 0}
        onClick={() => {
          return run(api.deleteClips(selected, false));
        }}
      >
        {t("delete")}
      </button>
      <button
        className={cx(styles, baseStyles, "icon")}
        disabled={selected.length === 0}
        onClick={() => {
          return run(api.deleteClips(selected, true));
        }}
      >
        {t("rippleDelete")}
      </button>
      <Menu
        label={t("menuClip")}
        deferred
      >
        <button
          disabled={selected.length < 2}
          onClick={() => {
            return run(api.mergeClipsIntoComposite(selected));
          }}
        >
          {t("mergeComposite")}
        </button>
        <button
          data-testid="create-compound-clip"
          disabled={selected.length === 0}
          onClick={() => {
            return selected[0] !== undefined && run(api.createCompoundClip(selected[0], selected));
          }}
        >
          {t("compoundClip")}
        </button>
        <Suspense fallback={null}>
          <LayerTemplates session={session} />
        </Suspense>
      </Menu>
      <Menu
        label={t("menuAnalyze")}
        deferred
      >
        <button
          disabled={selected.length !== 1}
          onClick={() => {
            return selected[0] !== undefined && run(api.detectSceneCutsForClip(selected[0]));
          }}
        >
          {t("detectChapters")}
        </button>
        <button
          onClick={() => {
            return run(api.detectHighlights());
          }}
        >
          {t("detectHighlightsAction")}
        </button>
        <Suspense fallback={null}>
          <TranscriptPanel session={session} />
          <SilenceReviewPanel />
        </Suspense>
      </Menu>
      <Menu label={t("menuAdd")}>
        <button
          onClick={() => {
            return run(api.addTrack("Video"));
          }}
        >
          {t("addVideoTrack")}
        </button>
        <button
          onClick={() => {
            return run(api.addTrack("Audio"));
          }}
        >
          {t("addAudioTrack")}
        </button>
        <button
          onClick={() => {
            return run(api.addTrack("Text"));
          }}
        >
          {t("addTextTrack")}
        </button>
        <button
          onClick={() => {
            return run(api.addTrack("Shape"));
          }}
        >
          {t("addShapeTrack")}
        </button>
        <button
          onClick={() => {
            return void addTextOverlay();
          }}
        >
          {t("addTextClip")}
        </button>
        <button
          onClick={() => {
            return void addShapeOverlay();
          }}
        >
          {t("addShapeClip")}
        </button>
        <button
          data-testid="draw-custom-shape"
          onClick={() => {
            return patchUi({ drawingShapePoints: [] });
          }}
        >
          {t("drawCustomShape")}
        </button>
      </Menu>
      <button
        className={cx(styles, baseStyles, "icon")}
        title={t("addMarker")}
        aria-label={t("addMarker")}
        onClick={() => {
          return run(api.addMarker());
        }}
      >
        <Icon name="flag" />
      </button>
      <button
        className={cx(styles, baseStyles, "icon")}
        title={t("previousMarker")}
        aria-label={t("previousMarker")}
        onClick={() => {
          return seekToMarker(markers, -1);
        }}
      >
        <Icon name="chevronLeft" />
      </button>
      <button
        className={cx(styles, baseStyles, "icon")}
        title={t("nextMarker")}
        aria-label={t("nextMarker")}
        onClick={() => {
          return seekToMarker(markers, 1);
        }}
      >
        <Icon name="chevronRight" />
      </button>
      <select
        className={cx(styles, baseStyles, "frame-rate-select")}
        aria-label={t("frameRateLabel")}
        title={t("frameRateLabel")}
        data-testid="frame-rate-select"
        value={frameRateKey(session.project.frame_rate)}
        onChange={(e) => {
          return run(api.setProjectFrameRate(frameRateFromKey(e.target.value)));
        }}
      >
        <option value={FREE_KEY}>{t("frameRateFree")}</option>
        {COMMON_FRAME_RATES.map((rate) => {
          return (
            <option
              key={frameRateKey(rate)}
              value={frameRateKey(rate)}
            >
              {frameRateText(rate)}
            </option>
          );
        })}
      </select>
      <span style={{ flex: 1 }} />
      <div className={cx(styles, baseStyles, editorStyles, "zoom-group")}>
        <button
          className={cx(styles, baseStyles, "icon")}
          title={t("zoomOut")}
          aria-label={t("zoomOut")}
          onClick={() => {
            return zoomBy(1 / 1.5);
          }}
        >
          <Icon name="minus" />
        </button>
        <button
          className={cx(styles, baseStyles, "icon")}
          title={t("zoomFit")}
          aria-label={t("zoomFit")}
          onClick={zoomToFit}
        >
          <Icon name="fit" />
        </button>
        <button
          className={cx(styles, baseStyles, "icon")}
          title={t("zoomIn")}
          aria-label={t("zoomIn")}
          onClick={() => {
            return zoomBy(1.5);
          }}
        >
          <Icon name="plus" />
        </button>
      </div>
    </div>
  );
}
