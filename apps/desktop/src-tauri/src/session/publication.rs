//! Publication keeps only the latest pending immutable snapshot. Diffing and IPC serialization
//! run outside the authoritative session lock; patches name the last published revision.
use crate::preview::mailbox::{self, Sender};
enum Task {
    Publish(Box<dyn FnOnce() + Send>),
    #[cfg(test)]
    Barrier(std::sync::mpsc::Sender<()>),
}
pub struct Publication {
    sender: Sender<Task>,
}
impl Default for Publication {
    fn default() -> Self {
        let (sender, receiver) = mailbox::channel::<Task>(1, |pending, task| {
            if matches!(task, Task::Publish(_)) {
                pending.retain(|task| !matches!(task, Task::Publish(_)));
            }
            Some(task)
        });
        std::thread::spawn(move || {
            while let Ok(task) = receiver.recv() {
                match task {
                    Task::Publish(task) => {
                        if std::panic::catch_unwind(std::panic::AssertUnwindSafe(task)).is_err() {
                            tracing::error!("session publication failed after worker panic");
                        }
                    }
                    #[cfg(test)]
                    Task::Barrier(reply) => {
                        let _ = reply.send(());
                    }
                }
            }
        });
        Self { sender }
    }
}
impl Publication {
    /// Waits for pending publications in the mock-runtime harness.
    #[cfg(test)]
    pub fn flush(&self) {
        let (reply, answer) = std::sync::mpsc::channel();
        self.sender
            .send(Task::Barrier(reply))
            .expect("publication worker alive");
        answer
            .recv_timeout(std::time::Duration::from_secs(120))
            .expect("publication completed");
    }
    pub fn submit(&self, task: impl FnOnce() + Send + 'static) {
        let _ = self.sender.send(Task::Publish(Box::new(task)));
    }
}
