//! Worker-owned decode loop with latest-only editor command delivery.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};

use crate::{
    DecodeSource, FrameMailbox, Generation, LatestIntent, PendingIntent, PublishedFrame, VideoFrame,
};

/// A replaceable preview command associated with an editor generation.
#[derive(Debug)]
pub enum PreviewCommand {
    Open { path: PathBuf },
    Seek { timestamp_secs: f64 },
}

/// The operation that failed in a worker-owned source session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkerOperation {
    Open,
    Seek,
    Decode,
}

/// A bounded diagnostic for the newest failed worker command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkerFailure {
    pub generation: Generation,
    pub operation: WorkerOperation,
    pub message: String,
}

/// A command was submitted after orderly worker shutdown.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkerSubmitError {
    Stopped,
}

#[derive(Debug, Default)]
struct CommandState {
    pending: LatestIntent<PreviewCommand>,
}

#[derive(Debug, Default)]
struct FrameState {
    mailbox: FrameMailbox,
}

/// A dedicated preview worker with one bounded command and frame slot.
///
/// The worker never blocks an editor caller on decode, and a newer command cancels decode in
/// progress. Its public methods are thread-safe; the native source itself remains worker-owned.
#[derive(Debug)]
pub struct PreviewWorker {
    commands: Arc<(Mutex<CommandState>, Condvar)>,
    frames: Arc<(Mutex<FrameState>, Condvar)>,
    failure: Arc<Mutex<Option<WorkerFailure>>>,
    cancel: Arc<AtomicBool>,
    stopped: Arc<AtomicBool>,
    join: Mutex<Option<JoinHandle<()>>>,
}

impl PreviewWorker {
    /// Starts an idle worker. It opens no media until an [`PreviewCommand::Open`] arrives.
    pub fn new() -> Self {
        let commands = Arc::new((Mutex::new(CommandState::default()), Condvar::new()));
        let frames = Arc::new((Mutex::new(FrameState::default()), Condvar::new()));
        let failure = Arc::new(Mutex::new(None));
        let cancel = Arc::new(AtomicBool::new(false));
        let stopped = Arc::new(AtomicBool::new(false));
        let join = thread::spawn(worker_loop(
            Arc::clone(&commands),
            Arc::clone(&frames),
            Arc::clone(&failure),
            Arc::clone(&cancel),
            Arc::clone(&stopped),
        ));
        Self {
            commands,
            frames,
            failure,
            cancel,
            stopped,
            join: Mutex::new(Some(join)),
        }
    }

    /// Replaces unstarted work and cooperatively cancels any decode already in progress.
    pub fn submit(
        &self,
        generation: Generation,
        command: PreviewCommand,
    ) -> Result<Option<PendingIntent<PreviewCommand>>, WorkerSubmitError> {
        if self.stopped.load(Ordering::Acquire) {
            return Err(WorkerSubmitError::Stopped);
        }
        self.cancel.store(true, Ordering::Release);
        let (state, wake) = &*self.commands;
        let replaced = state
            .lock()
            .expect("preview command mutex poisoned")
            .pending
            .replace(generation, command);
        wake.notify_one();
        Ok(replaced)
    }

    /// Returns the newest completed frame only when it matches `generation`.
    pub fn take_frame(&self, generation: Generation) -> Option<VideoFrame> {
        self.frames
            .0
            .lock()
            .expect("preview frame mutex poisoned")
            .mailbox
            .take_for(generation)
    }

    /// Waits until a matching frame arrives, shutdown occurs, or a newer generation supersedes it.
    pub fn wait_for_frame(&self, generation: Generation) -> Option<VideoFrame> {
        let (state, wake) = &*self.frames;
        let mut state = state.lock().expect("preview frame mutex poisoned");
        loop {
            if let Some(frame) = state.mailbox.take_for(generation) {
                return Some(frame);
            }
            if self.stopped.load(Ordering::Acquire) {
                return None;
            }
            state = wake.wait(state).expect("preview frame mutex poisoned");
        }
    }

    /// Takes the most recent worker failure, retaining a newer failure for its matching caller.
    pub fn take_failure(&self, generation: Generation) -> Option<WorkerFailure> {
        let mut failure = self.failure.lock().expect("preview failure mutex poisoned");
        match failure.as_ref().map(|failure| failure.generation) {
            Some(current) if current == generation => failure.take(),
            Some(current) if current < generation => {
                *failure = None;
                None
            }
            _ => None,
        }
    }

    /// Requests cooperative shutdown and joins the native-session owner.
    pub fn shutdown(&self) {
        if self.stopped.swap(true, Ordering::AcqRel) {
            return;
        }
        self.cancel.store(true, Ordering::Release);
        self.commands.1.notify_all();
        self.frames.1.notify_all();
        if let Some(join) = self
            .join
            .lock()
            .expect("preview join mutex poisoned")
            .take()
        {
            let _ = join.join();
        }
    }
}

impl Default for PreviewWorker {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for PreviewWorker {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn worker_loop(
    commands: Arc<(Mutex<CommandState>, Condvar)>,
    frames: Arc<(Mutex<FrameState>, Condvar)>,
    failure: Arc<Mutex<Option<WorkerFailure>>>,
    cancel: Arc<AtomicBool>,
    stopped: Arc<AtomicBool>,
) -> impl FnOnce() {
    move || {
        let mut source: Option<DecodeSource> = None;
        while let Some(pending) = take_command(&commands, &stopped) {
            cancel.store(false, Ordering::Release);
            let generation = pending.generation;
            let operation = match pending.value {
                PreviewCommand::Open { path } => {
                    source = None;
                    match DecodeSource::open(&path) {
                        Ok(opened) => {
                            source = Some(opened);
                            None
                        }
                        Err(error) => Some((WorkerOperation::Open, format!("{error:?}"))),
                    }
                }
                PreviewCommand::Seek { timestamp_secs } => match source.as_mut() {
                    Some(source) => source
                        .seek(timestamp_secs)
                        .err()
                        .map(|error| (WorkerOperation::Seek, format!("{error:?}"))),
                    None => Some((WorkerOperation::Seek, "no source is open".to_string())),
                },
            };
            if let Some((operation, message)) = operation {
                publish_failure(&failure, generation, operation, message);
                continue;
            }
            if cancel.load(Ordering::Acquire) {
                continue;
            }
            if let Some(source) = source.as_mut() {
                match source.next_frame(&cancel) {
                    Ok(Some(frame)) if !cancel.load(Ordering::Acquire) => {
                        let (state, wake) = &*frames;
                        state
                            .lock()
                            .expect("preview frame mutex poisoned")
                            .mailbox
                            .publish(PublishedFrame { generation, frame });
                        wake.notify_all();
                    }
                    Ok(_) => {}
                    Err(error) => publish_failure(
                        &failure,
                        generation,
                        WorkerOperation::Decode,
                        format!("{error:?}"),
                    ),
                }
            }
        }
    }
}

fn take_command(
    commands: &Arc<(Mutex<CommandState>, Condvar)>,
    stopped: &AtomicBool,
) -> Option<PendingIntent<PreviewCommand>> {
    let (state, wake) = &**commands;
    let mut state = state.lock().expect("preview command mutex poisoned");
    loop {
        if let Some(command) = state.pending.take() {
            return Some(command);
        }
        if stopped.load(Ordering::Acquire) {
            return None;
        }
        state = wake.wait(state).expect("preview command mutex poisoned");
    }
}

fn publish_failure(
    failure: &Mutex<Option<WorkerFailure>>,
    generation: Generation,
    operation: WorkerOperation,
    message: String,
) {
    *failure.lock().expect("preview failure mutex poisoned") = Some(WorkerFailure {
        generation,
        operation,
        message,
    });
}
