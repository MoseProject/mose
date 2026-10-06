import { cx } from "../cx";
import baseStyles from "../styles/base.module.css";
import styles from "./topbar.module.css";
import { shortcutLabel } from "../commands";
import { Menu } from "../editor/chrome/Menu";
import { t } from "../i18n";
import { api } from "../ipc";
import { patchUi, run } from "../store";
import type { SessionView } from "../types";
import { CommandItem } from "./CommandItem";

/** The application menus. */
export function MenuBar({
  session,
  screen,
  selected,
}: {
  session: SessionView | null;
  screen: string;
  selected: number[];
}) {
  return (
    <nav className={cx(styles, baseStyles, "menubar")}>
      {session && (
        <Menu
          variant="bar"
          label={t("menuFile")}
        >
          <CommandItem id="import" />
          <CommandItem id="save" />
          <CommandItem id="saveAs" />
          <CommandItem id="export" />
        </Menu>
      )}
      {session && (
        <Menu
          variant="bar"
          label={t("menuEdit")}
        >
          <CommandItem id="undo" />
          <CommandItem id="redo" />
          <CommandItem id="copy" />
          <CommandItem id="cut" />
          <CommandItem id="paste" />
          <CommandItem id="duplicate" />
          <CommandItem id="delete" />
          <CommandItem id="selectAll" />
        </Menu>
      )}
      <Menu
        variant="bar"
        label={t("menuView")}
      >
        <button
          data-testid="nav-home"
          aria-current={screen === "home" ? "page" : undefined}
          onClick={() => {
            return patchUi({ screen: "home" });
          }}
        >
          {t("home")}
        </button>
        <button
          data-testid="nav-prefs"
          data-shortcut={shortcutLabel("settings")}
          onClick={() => {
            return patchUi({ settingsOpen: true });
          }}
        >
          {t("preferences")}
        </button>
        <button
          data-testid="nav-watch"
          onClick={() => {
            return patchUi({ watchOpen: true });
          }}
        >
          {t("watchFolder")}
        </button>
        <CommandItem id="toggleGuides" />
        <CommandItem id="shortcuts" />
        <CommandItem id="palette" />
      </Menu>
      {session && (
        <Menu
          variant="bar"
          label={t("menuSequence")}
        >
          <button
            onClick={() => {
              return run(
                api.addSequence(t("sequence") + " " + (session.project.sequences.length + 1)),
              );
            }}
          >
            {t("addSequence")}
          </button>
          <CommandItem id="renameSequence" />
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
        </Menu>
      )}
      {session && (
        <Menu
          variant="bar"
          label={t("menuClip")}
        >
          <CommandItem id="split" />
          <CommandItem id="rippleDelete" />
          <button
            disabled={selected.length < 2}
            onClick={() => {
              return run(api.mergeClipsIntoComposite(selected));
            }}
          >
            {t("mergeComposite")}
          </button>
          <button
            disabled={selected.length === 0}
            onClick={() => {
              return run(api.createCompoundClip(selected[0], selected));
            }}
          >
            {t("compoundClip")}
          </button>
          <CommandItem id="copyFormat" />
          <CommandItem id="pasteFormat" />
        </Menu>
      )}
      {session && (
        <Menu
          variant="bar"
          label={t("markers")}
        >
          <CommandItem id="addMarker" />
          <CommandItem id="prevMarker" />
          <CommandItem id="nextMarker" />
          <CommandItem id="markIn" />
          <CommandItem id="markOut" />
          <CommandItem id="clearMarks" />
          <CommandItem id="deleteRange" />
        </Menu>
      )}
      {session && (
        <Menu
          variant="bar"
          label={t("menuGraphics")}
        >
          <CommandItem id="addTextClip" />
          <CommandItem id="addShapeClip" />
          <button
            onClick={() => {
              return patchUi({ drawingShapePoints: [] });
            }}
          >
            {t("drawCustomShape")}
          </button>
        </Menu>
      )}
    </nav>
  );
}
