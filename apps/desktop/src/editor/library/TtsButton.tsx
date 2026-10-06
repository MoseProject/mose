import homeStyles from "../../screens/home.module.css";
import { cx } from "../../cx";
import baseStyles from "../../styles/base.module.css";
import styles from "./library.module.css";
import { useState } from "react";
import { t } from "../../i18n";
import { api } from "../../ipc";
import { run } from "../../store";
import { Dialog } from "../chrome/Dialog";

export function TtsButton() {
  const [text, setText] = useState<string | null>(null);
  if (text === null) {
    return (
      <button
        className={cx(styles, baseStyles, homeStyles, "icon")}
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
      <div className={cx(styles, baseStyles, "modal-actions")}>
        <button
          className={cx(styles, baseStyles, "icon")}
          onClick={() => {
            return setText(null);
          }}
        >
          {t("cancel")}
        </button>
        <button
          className={cx(styles, baseStyles, "icon primary")}
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
