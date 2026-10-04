import { addShapeOverlay, addTextOverlay, seekToMarker } from "./editing";
import logo from "./assets/logo.png";
import { Icon } from "./editor/Icon";
import { Menu } from "./editor/Menu";
import { formatTime, t } from "./i18n";
import { api, exportActiveSequence } from "./ipc";
import { exportsStore, patchUi, previewStore, run, uiStore, useStore } from "./store";
import type { SessionView } from "./types";

const activeTimeline = (session: SessionView) => {
  return session.project.sequences[session.project.active_sequence].timeline;
};

/** Brand, project/sequence, the application menus, the playhead readout and the global actions. */
export function TopBar({ session }: { session: SessionView | null }) {
  const ui = useStore(uiStore);
  const jobs = useStore(exportsStore);
  const activeExports = jobs.filter((job) => {
    return (
      job.status === "Queued" ||
      (typeof job.status === "object" && ("Rendering" in job.status || "Paused" in job.status))
    );
  }).length;
  const sequenceName = session?.project.sequences[session.project.active_sequence]?.name;
  const selected = ui.selectedClipIds;
  const markers = session ? activeTimeline(session).markers : [];
  const startExport = () => {
    patchUi({ exportQueueOpen: true });
    void run(
      exportActiveSequence(() => {
        return window.confirm(t("exportOverwriteConfirm"));
      }),
    );
  };

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
            <button
              onClick={() => {
                return run(api.importMediaDialog());
              }}
            >
              {t("importFiles")}
            </button>
            <button
              onClick={() => {
                return run(api.saveProject(false));
              }}
            >
              {t("save")}
            </button>
            <button
              onClick={() => {
                return run(api.saveProject(true));
              }}
            >
              {t("saveAs")}
            </button>
            <button onClick={startExport}>{t("export")}</button>
          </Menu>
        )}
        {session && (
          <Menu
            variant="bar"
            label={t("menuEdit")}
          >
            <button
              disabled={!session.canUndo}
              onClick={() => {
                return run(api.undo());
              }}
            >
              {t("undo")}
            </button>
            <button
              disabled={!session.canRedo}
              onClick={() => {
                return run(api.redo());
              }}
            >
              {t("redo")}
            </button>
            <button
              disabled={selected.length === 0}
              onClick={() => {
                return run(api.copyClip(selected[0]));
              }}
            >
              {t("menuCopy")}
            </button>
            <button
              disabled={selected.length === 0}
              onClick={() => {
                return run(api.cutClip(selected[0]));
              }}
            >
              {t("menuCut")}
            </button>
            <button
              onClick={() => {
                return void run(api.pasteClip()).then((ids) => {
                  return ids && ids.length > 0 && patchUi({ selectedClipIds: ids });
                });
              }}
            >
              {t("menuPaste")}
            </button>
            <button
              disabled={selected.length === 0}
              onClick={() => {
                return run(api.deleteClips(selected, false));
              }}
            >
              {t("delete")}
            </button>
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
            aria-current={ui.screen === "prefs" ? "page" : undefined}
            onClick={() => {
              return patchUi({ screen: "prefs" });
            }}
          >
            {t("preferences")}
          </button>
          <button
            data-testid="nav-watch"
            aria-current={ui.screen === "watch" ? "page" : undefined}
            onClick={() => {
              return patchUi({ screen: "watch" });
            }}
          >
            {t("watchFolder")}
          </button>
        </Menu>
        {session && (
          <Menu
            variant="bar"
            label={t("menuSequence")}
          >
            <button
              onClick={() => {
                return run(
                  api.addSequence(`${t("sequence")} ${session.project.sequences.length + 1}`),
                );
              }}
            >
              {t("addSequence")}
            </button>
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
            <button
              onClick={() => {
                return run(api.splitAtPlayhead(selected));
              }}
            >
              {t("split")}
            </button>
            <button
              disabled={selected.length === 0}
              onClick={() => {
                return run(api.deleteClips(selected, true));
              }}
            >
              {t("rippleDelete")}
            </button>
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
          </Menu>
        )}
        {session && (
          <Menu
            variant="bar"
            label={t("markers")}
          >
            <button
              onClick={() => {
                return run(api.addMarker());
              }}
            >
              {t("addMarker")}
            </button>
            <button
              onClick={() => {
                return seekToMarker(markers, -1);
              }}
            >
              {t("previousMarker")}
            </button>
            <button
              onClick={() => {
                return seekToMarker(markers, 1);
              }}
            >
              {t("nextMarker")}
            </button>
          </Menu>
        )}
        {session && (
          <Menu
            variant="bar"
            label={t("menuGraphics")}
          >
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
            title={t("undo")}
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
            title={t("redo")}
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
            title={t("save")}
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
            className={`icon ${ui.exportQueueOpen ? "active" : ""}`}
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
            onClick={startExport}
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
        title={t("preferences")}
        onClick={() => {
          return patchUi({ screen: "prefs" });
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

/** Playhead timecode plus the source's frame rate and resolution, read-only. */
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
      <span className="timecode mono">{formatTime(tick.playheadSecs)}</span>
      {source?.fps != null && <span className="muted mono">{Math.round(source.fps)} FPS</span>}
      {source?.resolution && (
        <span className="muted mono">
          {source.resolution[0]} × {source.resolution[1]}
        </span>
      )}
    </div>
  );
}
