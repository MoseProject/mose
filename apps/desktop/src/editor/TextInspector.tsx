import { t } from "../i18n";
import { api } from "../ipc";
import { run } from "../store";
import type { TextClip, TextClipPatch } from "../types";
import { TextField } from "./OverlayFields";
import { TypographySection } from "./textInspector/TypographySection";
import { BackgroundSection } from "./textInspector/BackgroundSection";
import { PositionSection } from "./textInspector/PositionSection";
import { KeyframesSection } from "./textInspector/KeyframesSection";
import { TimingSection } from "./textInspector/TimingSection";

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
        className="panel-title"
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
