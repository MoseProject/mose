//! Sequence-tab edits. Undo snapshots intentionally remain sequence-local: changing the active
//! sequence clears them, while reordering tabs preserves them because the active sequence's
//! identity and its snapshot history have not changed.

use tauri::{AppHandle, Runtime};

use super::with_session;

#[tauri::command]
pub fn add_sequence<R: Runtime>(app: AppHandle<R>, name: String) -> Result<(), String> {
    with_session(&app, |session| {
        session.project.new_sequence(name);
        reset_sequence_context(session);
        Ok(())
    })
}

#[tauri::command]
pub fn select_sequence<R: Runtime>(app: AppHandle<R>, index: usize) -> Result<(), String> {
    with_session(&app, |session| {
        if index >= session.project.sequences.len() {
            return Err("Sequence does not exist.".into());
        }
        if index != session.project.active_sequence {
            session.project.active_sequence = index;
            reset_sequence_context(session);
        }
        Ok(())
    })
}

#[tauri::command]
pub fn duplicate_sequence<R: Runtime>(
    app: AppHandle<R>,
    index: usize,
    name: String,
) -> Result<(), String> {
    with_session(&app, |session| {
        session
            .project
            .duplicate_sequence(index, name)
            .ok_or_else(|| "Sequence does not exist.".to_string())?;
        reset_sequence_context(session);
        Ok(())
    })
}

#[tauri::command]
pub fn rename_sequence<R: Runtime>(
    app: AppHandle<R>,
    sequence_id: u64,
    name: String,
) -> Result<(), String> {
    with_session(&app, |session| {
        let sequence = session
            .project
            .sequences
            .iter_mut()
            .find(|sequence| sequence.id == sequence_id)
            .ok_or_else(|| "Sequence does not exist.".to_string())?;
        if sequence.name != name {
            sequence.name = name;
            session.dirty = true;
        }
        Ok(())
    })
}

#[tauri::command]
pub fn remove_sequence<R: Runtime>(app: AppHandle<R>, sequence_id: u64) -> Result<(), String> {
    with_session(&app, |session| {
        let index = session
            .project
            .sequences
            .iter()
            .position(|sequence| sequence.id == sequence_id)
            .ok_or_else(|| "Sequence does not exist.".to_string())?;
        if session.project.sequences.len() <= 1 {
            return Err("The last sequence cannot be removed.".into());
        }
        if !session
            .project
            .sequences_referencing_as_compound_clip(sequence_id)
            .is_empty()
        {
            return Err("A sequence used as a compound clip cannot be removed.".into());
        }
        let active_id = session.project.active_sequence().id;
        session.project.remove_sequence(index);
        session.dirty = true;
        if session.project.active_sequence().id != active_id {
            reset_sequence_context(session);
        }
        Ok(())
    })
}

#[tauri::command]
pub fn move_sequence<R: Runtime>(
    app: AppHandle<R>,
    from_index: usize,
    target_index: usize,
) -> Result<(), String> {
    with_session(&app, |session| {
        if !session.project.move_sequence(from_index, target_index) {
            return Err("Sequence cannot be moved.".into());
        }
        session.dirty = true;
        Ok(())
    })
}

/// Mirrors `ui::App::reset_sequence_context` for the backend-owned fields. The webview clears
/// its transient selection when the published active sequence id changes.
fn reset_sequence_context(session: &mut crate::state::Session) {
    session.undo.clear();
    session.dirty = true;
}

#[cfg(test)]
#[path = "sequences_test.rs"]
mod tests;
