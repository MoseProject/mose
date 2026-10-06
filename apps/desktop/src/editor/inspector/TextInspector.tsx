import { cx } from "../../cx";
import baseStyles from "../../styles/base.module.css";
import styles from "./inspector.module.css";
import { t } from "../../i18n";
import { api } from "../../ipc";
import { run } from "../../store";
import type { TextClip, TextClipPatch } from "../../types";
import { TextField } from "./OverlayFields";
import { TypographySection } from "./text/TypographySection";
import { BackgroundSection } from "./text/BackgroundSection";
import { PositionSection } from "./text/PositionSection";
import { KeyframesSection } from "./text/KeyframesSection";
import { TimingSection } from "./text/TimingSection";

// Inspector section for a selected text overlay -- the egui properties panel's text controls
// (`crates/ui/src/screens/editor/properties_panel/text_clip.rs`). Each change is one
// `update_text_clip` call, so one undo step; the preview engine reopens with the new overlay.

export function TextInspector({ clip }: { clip: TextClip }) {
  const commit = (patch: TextClipPatch) => {
    return void run(api.updateTextClip(clip.id, patch));
  };

  return (
    <>
      <div
        className={cx(styles, baseStyles, "panel-title")}
        style={{ border: 0, padding: 0 }}
      >
        {t("text")}
      </div>
      <TextField
        label={t("text")}
        value={clip.text}
        placeholder={t("textContentHint")}
        onCommit={(text) => {
          return commit({ text });
        }}
      />
      <TypographySection
        clip={clip}
        commit={commit}
      />
      <BackgroundSection
        clip={clip}
        commit={commit}
      />
      <PositionSection
        clip={clip}
        commit={commit}
      />
      <KeyframesSection
        clip={clip}
        commit={commit}
      />
      <TimingSection
        clip={clip}
        commit={commit}
      />
    </>
  );
}
