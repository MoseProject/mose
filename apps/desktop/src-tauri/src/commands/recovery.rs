// Copyright (C) 2026 by Lucas Gomes <lucasgsm88@gmail.com>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License

//! Unsaved work that an earlier launch left behind (see [`crate::recovery`]).

use serde::Serialize;
use tauri::{AppHandle, Runtime};

use crate::recovery;

/// One recoverable session, as the home screen lists it.
#[derive(Serialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryView {
    pub id: String,
    pub project_name: String,
    pub saved_unix_secs: u64,
}

#[tauri::command]
pub fn list_recoveries<R: Runtime>(app: AppHandle<R>) -> Vec<RecoveryView> {
    recovery::candidates(&app)
        .into_iter()
        .map(|candidate| RecoveryView {
            id: candidate.id,
            project_name: candidate.project_name,
            saved_unix_secs: candidate.saved_unix_secs,
        })
        .collect()
}

#[tauri::command]
pub fn recover_session<R: Runtime>(app: AppHandle<R>, id: String) -> Result<(), String> {
    recovery::recover(&app, &id)
}

#[tauri::command]
pub fn discard_recovery<R: Runtime>(app: AppHandle<R>, id: String) -> Result<(), String> {
    recovery::discard(&app, &id)
}
