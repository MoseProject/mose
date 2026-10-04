import { useEffect, useState } from "react";
import { formatTime, t } from "../i18n";
import { api } from "../ipc";
import { previewStore, run, uiStore, useStore } from "../store";
import {
  activeTimeline,
  type SessionView,
  type TranscriptDocument,
  type TranscriptHit,
  type TranscriptWord,
} from "../types";
import { Dialog } from "./Dialog";

/** Transcript search/seek popup -- egui's `App::show_transcript_panel` narrow modal shape. Two
 *  modes: off (default) filters the selected clip's own transcript; on (with a query) searches
 *  every asset in the project library that has a saved transcript. */
export function TranscriptPanel({ session }: { session: SessionView }) {
  const [open, setOpen] = useState(false);
  const [search, setSearch] = useState("");
  const [searchProject, setSearchProject] = useState(false);
  const [loadedDocument, setDocument] = useState<TranscriptDocument | null>(null);
  const [loadedHits, setHits] = useState<TranscriptHit[]>([]);
  const ui = useStore(uiStore);
  const preview = useStore(previewStore);

  const clip =
    ui.selectedClipIds.length === 1
      ? activeTimeline(session.project)
          .tracks.flatMap((track) => {
            return track.clips;
          })
          .find((c) => {
            return c.id === ui.selectedClipIds[0];
          })
      : undefined;

  const clipAssetId = clip?.asset_id;
  useEffect(() => {
    if (!open || searchProject || clipAssetId === undefined) {
      return;
    }
    void run(api.getTranscript(clipAssetId)).then((doc) => {
      setDocument(doc ?? null);
    });
  }, [open, searchProject, clipAssetId]);

  const query = search.trim().toLowerCase();
  const document = open && !searchProject && clipAssetId !== undefined ? loadedDocument : null;
  const hits = open && searchProject && query.length > 0 ? loadedHits : [];
  useEffect(() => {
    if (!open || !searchProject || query.length === 0) {
      return;
    }
    void run(api.searchTranscripts(query)).then((result) => {
      setHits(result ?? []);
    });
  }, [open, searchProject, query]);

  if (!open) {
    return (
      <button
        className="icon"
        onClick={() => {
          return setOpen(true);
        }}
      >
        {t("transcriptPanelTitle")}
      </button>
    );
  }

  const currentSourceSecs = clip
    ? clip.frozen
      ? clip.source_in_secs
      : clip.source_in_secs +
        (preview.playheadSecs - clip.start_secs) * Math.max(clip.speed_factor, 0.01)
    : undefined;
  const currentWordId = document?.words.find((w) => {
    return (
      currentSourceSecs !== undefined &&
      currentSourceSecs >= w.start_secs &&
      currentSourceSecs < w.end_secs
    );
  })?.id;

  const seek = (assetId: number, sourceSecs: number) => {
    return void run(api.seekToTranscriptWord(assetId, sourceSecs));
  };

  return (
    <Dialog
      title={t("transcriptPanelTitle")}
      onClose={() => {
        setOpen(false);
      }}
    >
      <input
        autoFocus
        value={search}
        placeholder={
          searchProject ? t("transcriptProjectSearchHint") : t("transcriptPanelSearchHint")
        }
        onChange={(e) => {
          return setSearch(e.target.value);
        }}
      />
      <label
        className="check"
        style={{ marginTop: 6 }}
      >
        <input
          type="checkbox"
          checked={searchProject}
          onChange={(e) => {
            return setSearchProject(e.target.checked);
          }}
        />
        {t("transcriptSearchProject")}
      </label>
      <div style={{ maxHeight: 320, overflowY: "auto", marginTop: 8 }}>
        {searchProject ? (
          hits.length === 0 ? (
            <p className="muted">{t("transcriptProjectEmpty")}</p>
          ) : (
            groupByAsset(hits).map(([assetId, assetName, words]) => {
              return (
                <div key={assetId}>
                  <div
                    className="muted"
                    style={{ marginTop: 6 }}
                  >
                    {assetName}
                  </div>
                  {words.map((word) => {
                    return (
                      <WordButton
                        key={word.id}
                        word={word}
                        active={word.id === currentWordId}
                        label={`${formatTime(word.start_secs)} - ${word.text}`}
                        onClick={() => {
                          return seek(assetId, word.start_secs);
                        }}
                      />
                    );
                  })}
                </div>
              );
            })
          )
        ) : !clip ? (
          <p className="muted">{t("transcriptPanelNoClip")}</p>
        ) : !document ? (
          <p className="muted">{t("transcriptPanelNoTranscript")}</p>
        ) : (
          document.words
            .filter((w) => {
              return query.length === 0 || w.text.toLowerCase().includes(query);
            })
            .map((word) => {
              return (
                <WordButton
                  key={word.id}
                  word={word}
                  active={word.id === currentWordId}
                  label={word.text}
                  onClick={() => {
                    return seek(document.asset_id, word.start_secs);
                  }}
                />
              );
            })
        )}
      </div>
      <div className="modal-actions">
        <button
          className="icon"
          onClick={() => {
            return setOpen(false);
          }}
        >
          {t("cancel")}
        </button>
      </div>
    </Dialog>
  );
}

function groupByAsset(hits: TranscriptHit[]): [number, string, TranscriptWord[]][] {
  const order: number[] = [];
  const byAsset = new Map<number, { name: string; words: TranscriptWord[] }>();
  for (const hit of hits) {
    if (!byAsset.has(hit.assetId)) {
      order.push(hit.assetId);
      byAsset.set(hit.assetId, { name: hit.assetName, words: [] });
    }
    byAsset.get(hit.assetId)!.words.push(hit.word);
  }
  return order.map((id) => {
    return [id, byAsset.get(id)!.name, byAsset.get(id)!.words];
  });
}

function WordButton(props: {
  word: TranscriptWord;
  active: boolean;
  label: string;
  onClick: () => void;
}) {
  return (
    <button
      className={`icon ${props.active ? "active" : ""}`}
      title={`${formatTime(props.word.start_secs)} (${(props.word.confidence * 100).toFixed(0)}%)`}
      style={{
        margin: "2px 4px 2px 0",
        opacity: props.word.confidence < 0.5 && !props.active ? 0.6 : 1,
      }}
      onClick={props.onClick}
    >
      {props.label}
    </button>
  );
}
