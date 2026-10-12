//! Compound clips for the preview. A compound clip's nested sequence is rendered to a video file,
//! which takes seconds; doing that on the playback thread froze the preview at every edit made
//! inside one. Here the render is made in the background, and until it is done the preview shows
//! the previous render (or nothing, for a compound clip that has never been rendered).

use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::Arc;

use avcore::nested_sequence::{
    cache_dir_for_project, cached_nested_assets, materialize_nested_sequences,
    nested_sequences_are_current, NestedSequenceCache,
};
use avcore::timeline::Timeline;
use avcore::{MediaAsset, Project};

/// What a background render hands back: the cache as it ended up (renders that succeeded stay in
/// it even when a later one failed).
struct Finished {
    cache: HashMap<u64, NestedSequenceCache>,
}

#[derive(Default)]
pub(super) struct NestedRenderer {
    cache: HashMap<u64, NestedSequenceCache>,
    running: Option<Receiver<Finished>>,
    /// The newest project that arrived while a render was running; it is rendered next.
    waiting: Option<Arc<Project>>,
}

impl NestedRenderer {
    /// The assets of the compound clips under `timeline` to show now. When every render is current
    /// that is a few comparisons; otherwise the renders there are (possibly stale) are returned and
    /// a background render of `project` is started or queued.
    pub(super) fn assets_for(
        &mut self,
        project: &Arc<Project>,
        timeline: &Timeline,
    ) -> HashMap<u64, MediaAsset> {
        if nested_sequences_are_current(project, timeline, &self.cache) {
            return materialize_nested_sequences(
                project,
                timeline,
                &cache_dir_for_project(project),
                &mut self.cache,
            )
            .unwrap_or_default();
        }
        self.request(project);
        cached_nested_assets(project, timeline, &self.cache)
    }

    /// Whether a render is going on, which is when the loop should look for its end soon.
    pub(super) fn is_running(&self) -> bool {
        self.running.is_some()
    }

    /// Takes in a render that has finished and starts the one that was waiting. True when the
    /// scene should be built again from the new renders.
    pub(super) fn poll(&mut self) -> bool {
        let Some(running) = &self.running else {
            return false;
        };
        let finished = match running.try_recv() {
            Ok(finished) => finished,
            Err(TryRecvError::Empty) => return false,
            // The thread is gone without an answer: whatever there was stays as it is.
            Err(TryRecvError::Disconnected) => {
                self.running = None;
                self.start_waiting();
                return false;
            }
        };
        self.running = None;
        self.cache = finished.cache;
        self.start_waiting();
        // Even a render that failed part way may have made some of its sequences.
        true
    }

    fn request(&mut self, project: &Arc<Project>) {
        if self.running.is_some() {
            self.waiting = Some(Arc::clone(project));
        } else {
            self.start(Arc::clone(project));
        }
    }

    fn start_waiting(&mut self) {
        if let Some(project) = self.waiting.take() {
            self.start(project);
        }
    }

    fn start(&mut self, project: Arc<Project>) {
        let (answer, receiver) = mpsc::channel();
        let mut cache = self.cache.clone();
        self.running = Some(receiver);
        std::thread::Builder::new()
            .name("oca-nested-render".into())
            .spawn(move || {
                let timeline = project.timeline().clone();
                let result = materialize_nested_sequences(
                    &project,
                    &timeline,
                    &cache_dir_for_project(&project),
                    &mut cache,
                );
                if let Err(error) = &result {
                    tracing::warn!(%error, "failed to render a compound clip for the preview");
                }
                let _ = answer.send(Finished { cache });
            })
            .expect("spawning the compound clip render thread");
    }
}

#[cfg(test)]
#[path = "nested_render_test.rs"]
mod tests;
