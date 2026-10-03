//! The native file dialogs, behind one seam so command tests can answer them without a window.
//! The real pickers (`rfd`) exist only outside tests; under `cfg(test)` every picker pops the
//! next answer a test queued with [`scripted::answer`] and reports a cancelled dialog when none
//! is queued.

#[cfg(not(test))]
use std::path::PathBuf;

#[cfg(not(test))]
pub fn pick_file(name: &str, extensions: &[&str]) -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter(name, extensions)
        .pick_file()
}

#[cfg(not(test))]
pub fn pick_files(name: &str, extensions: &[&str]) -> Option<Vec<PathBuf>> {
    rfd::FileDialog::new()
        .add_filter(name, extensions)
        .pick_files()
}

#[cfg(not(test))]
pub fn save_file(name: &str, extensions: &[&str], file_name: &str) -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter(name, extensions)
        .set_file_name(file_name)
        .save_file()
}

#[cfg(not(test))]
pub fn pick_folder() -> Option<PathBuf> {
    rfd::FileDialog::new().pick_folder()
}

#[cfg(test)]
pub use scripted::{pick_file, pick_files, pick_folder, save_file};

#[cfg(test)]
pub mod scripted {
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::path::PathBuf;

    thread_local! {
        static ANSWERS: RefCell<VecDeque<Option<Vec<PathBuf>>>> = const { RefCell::new(VecDeque::new()) };
    }

    /// Queues what the next dialog on this thread returns: `Some(paths)` for a pick, `None`
    /// for a cancelled dialog.
    pub fn answer(paths: Option<Vec<PathBuf>>) {
        ANSWERS.with(|queue| queue.borrow_mut().push_back(paths));
    }

    fn next() -> Option<Vec<PathBuf>> {
        ANSWERS
            .with(|queue| queue.borrow_mut().pop_front())
            .flatten()
    }

    pub fn pick_file(_name: &str, _extensions: &[&str]) -> Option<PathBuf> {
        next().and_then(|paths| paths.into_iter().next())
    }

    pub fn pick_files(_name: &str, _extensions: &[&str]) -> Option<Vec<PathBuf>> {
        next()
    }

    pub fn save_file(_name: &str, _extensions: &[&str], _file_name: &str) -> Option<PathBuf> {
        next().and_then(|paths| paths.into_iter().next())
    }

    pub fn pick_folder() -> Option<PathBuf> {
        next().and_then(|paths| paths.into_iter().next())
    }
}
