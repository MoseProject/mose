import logo from "./assets/logo.png";
import { findCommand, shortcutLabel, withShortcut } from "./commands";
import { Icon } from "./editor/Icon";
import { Menu } from "./editor/Menu";
import { formatTimecode, t } from "./i18n";
import { api } from "./ipc";
import { keymapStore } from "./keymap";
import { exportsStore, patchUi, previewStore, run, uiStore, useStore } from "./store";
import type { SessionView } from "./types";

/** A menu entry bound to a command: its label, key binding, enabled state and action all come
 *  from the command table. */
function CommandItem({ id }: { id: string }) {
  const command = findCommand(id);
  if (!command) {
    return null;
  }
  return (
    <button
      data-shortcut={shortcutLabel(id)}
      disabled={command.enabled ? !command.enabled() : false}
      onClick={() => {
        void command.run();
      }}
    >
      {t(command.label)}
    </button>
  );
}

/** Brand, project/sequence, the application menus, the playhead readout and the global actions. */
export function TopBar({ session }: { session: SessionView | null }) {
  const ui = useStore(uiStore);
  useStore(keymapStore); // shortcut hints follow the user's changes
  const jobs = useStore(exportsStore);
  const activeExports = jobs.filter((job) => {
    return (
      job.status === "Queued" ||
      (typeof job.status === "object" && ("Rendering" in job.status || "Paused" in job.status))
    );
  }).length;
  const sequenceName = session?.project.sequences[session.project.active_sequence]?.name;
  const selected = ui.selectedClipIds;

  return (
    <header className="topbar">
      <div className="brand-box">
        <img
          className="brand"
          src={logo}
          alt="oca"
          width={24}
          height={24}
        />
      </div>
      {session && (
        <div className="crumbs">
          <span className="project-name">
            {t("projectLabel")}: {session.project.name}
          </span>
          {session.dirty && (
            <span
              className="dirty"
              title={t("unsaved")}
              aria-label={t("unsaved")}
            />
          )}
          {sequenceName && (
            <span className="muted">
              / {t("sequence")}: {sequenceName}
            </span>
          )}
        </div>
      )}
      <nav className="menubar">
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
            aria-current={ui.screen === "home" ? "page" : undefined}
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
      <span className="spacer" />
      {session && (
        <>
          <Readout session={session} />
          <button
            data-testid="undo"
            className="icon"
            aria-label={t("undo")}
            title={withShortcut(t("undo"), "undo")}
            disabled={!session.canUndo}
            onClick={() => {
              return run(api.undo());
            }}
          >
            <Icon
              name="undo"
              size={16}
            />
          </button>
          <button
            data-testid="redo"
            className="icon"
            aria-label={t("redo")}
            title={withShortcut(t("redo"), "redo")}
            disabled={!session.canRedo}
            onClick={() => {
              return run(api.redo());
            }}
          >
            <Icon
              name="redo"
              size={16}
            />
          </button>
          <button
            data-testid="save-project"
            className="icon"
            aria-label={t("save")}
            title={withShortcut(t("save"), "save")}
            onClick={() => {
              return run(api.saveProject(false));
            }}
          >
            <Icon
              name="save"
              size={16}
            />
          </button>
          <button
            data-testid="export-queue-toggle"
            className={"icon " + (ui.exportQueueOpen ? "active" : "")}
            aria-label={t("exportQueue")}
            title={t("exportQueue")}
            aria-expanded={ui.exportQueueOpen}
            onClick={() => {
              return patchUi({ exportQueueOpen: !ui.exportQueueOpen });
            }}
          >
            <Icon
              name="queue"
              size={16}
            />
            {activeExports > 0 && <span className="badge">{activeExports}</span>}
          </button>
          <button
            data-testid="export"
            className="primary export"
            title={withShortcut(t("export"), "export")}
            onClick={() => {
              patchUi({ exportDialogOpen: true });
            }}
          >
            <Icon
              name="upload"
              size={14}
            />{" "}
            {t("export")}
          </button>
        </>
      )}
      <button
        className="icon"
        aria-label={t("preferences")}
        title={withShortcut(t("preferences"), "settings")}
        onClick={() => {
          return patchUi({ settingsOpen: true });
        }}
      >
        <Icon
          name="settings"
          size={17}
        />
      </button>
    </header>
  );
}

/** Playhead timecode (hours:minutes:seconds:frames) plus the source's frame rate and
 *  resolution, read-only. */
function Readout({ session }: { session: SessionView }) {
  const tick = useStore(previewStore);
  const source = session.project.media_library.find((asset) => {
    return asset.kind === "Video" && asset.resolution;
  });
  return (
    <div
      className="readout"
      aria-label={t("timecode")}
    >
      <span className="timecode mono">{formatTimecode(tick.playheadSecs, source?.fps ?? 30)}</span>
      {source?.fps != null && <span className="muted mono">{Math.round(source.fps)} FPS</span>}
      {source?.resolution && (
        <span className="muted mono">
          {source.resolution[0]} × {source.resolution[1]}
        </span>
      )}
    </div>
  );
}
