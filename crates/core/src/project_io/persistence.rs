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
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

//! Saves/loads any `Serialize + DeserializeOwned` value using oca's binary formats: a small
//! format-specific magic/version header, then gzip-compressed MessagePack (struct-map mode).
//! Struct-map mode keeps field names in the encoded bytes (unlike MessagePack's default compact/array
//! mode), so `#[serde(default)]` on a field added after a value was last saved still works the
//! same way it did for JSON — nearly every past feature in this codebase relies on that to let
//! an older saved [`Project`] keep loading. The (de)serialization core
//! ([`to_ocproj_bytes`]/[`from_ocproj_bytes`]) is generic — `ui`'s `PrefsState` reuses the exact
//! same framing for its own `prefs.oc`, not just `Project`. Export queues use the same payload
//! encoding with their own `OCQU` identity through [`to_ocqueue_bytes`]/
//! [`from_ocqueue_bytes`]; transcript-document sidecars (`crate::transcript`, CF-01) use `OCTR`
//! through [`to_octr_bytes`]/[`from_octr_bytes`]; semantic-search index sidecars
//! (`crate::semantic_index`, CF-08) use `OCSI` through [`to_ocsi_bytes`]/[`from_ocsi_bytes`].
//! [`save_project_to_file`]/
//! [`load_project_from_file`] are thin `Project`-specific file-touching shells around the
//! project framing.

use std::fs;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::project::Project;

/// Identifies an `.ocproj` file before we even try to gzip/msgpack-decode it, so a
/// wrong-file/corrupt-file error says exactly that instead of surfacing an opaque gzip or
/// msgpack failure.
const PROJECT_MAGIC: &[u8; 4] = b"OCPJ";
/// Identifies a compressed export-queue file (`queue.ocqueue`). Keeping this distinct from
/// [`PROJECT_MAGIC`] makes wrong-file errors deterministic before deserialization.
const QUEUE_MAGIC: &[u8; 4] = b"OCQU";
/// Identifies a compressed transcript-document sidecar file (`crate::transcript`, CF-01).
const TRANSCRIPT_MAGIC: &[u8; 4] = b"OCTR";
/// Identifies a compressed semantic-search index sidecar file (`crate::semantic_index`, CF-08).
const SEMANTIC_INDEX_MAGIC: &[u8; 4] = b"OCSI";
/// Layout of a project file: `OCPJ`, this version, the payload size as a little-endian `u32`, then
/// the LZ4-compressed `postcard` encoding of the [`Project`]. Projects are written this way
/// instead of through the generic MessagePack framing because that spends most of its time
/// (about 70% of a save) writing and reading each field's name for every clip; positional
/// `postcard` is several times faster to write and read and far smaller before compression.
/// The price is that the encoding is positional: any change to the fields of a type inside
/// [`Project`] changes the bytes, so such a change must bump this version, and files from other
/// versions are refused with a clear error rather than misread.
const PROJECT_FORMAT_VERSION: u8 = 4;
/// The previous project layout, still readable: it is the current one without the trailing
/// `frame_rate: Option<FrameRate>`, so [`upgrade_v3`] only has to append the encoding of `None`.
const PROJECT_FORMAT_VERSION_V3: u8 = 3;
/// Postcard's encoding of `Option::None`.
const POSTCARD_NONE: u8 = 0;
/// Bumped only if the on-disk layout (header framing, not the `Project` schema itself —
/// that's handled by `#[serde(default)]`) ever needs to change.
const FORMAT_VERSION: u8 = 1;

/// Upper bound on the *decompressed* size of an `.ocproj`/`.ocqueue`/`prefs.oc` payload —
/// without this, [`from_framed_bytes`] would happily `read_to_end` an attacker-crafted or
/// corrupted file's gzip stream with no limit, and gzip routinely achieves 1000:1+ compression
/// ratios on repetitive input, so a tiny file could exhaust memory (a "gzip bomb" DoS) well
/// before MessagePack decoding ever gets a chance to reject it. A real project's own data is
/// plain numbers/strings/enums (paths, not embedded media) even for a large timeline with many
/// keyframes, so 256 MiB is generous headroom, not a tight fit.
const MAX_DECOMPRESSED_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Debug)]
pub enum PersistError {
    Io(std::io::Error),
    Encode(rmp_serde::encode::Error),
    Decode(rmp_serde::decode::Error),
    /// The positional project encoding failed ([`to_project_bytes`]/[`from_project_bytes`]).
    Project(postcard::Error),
    /// Bytes that don't start with the expected magic, or whose version byte isn't
    /// [`FORMAT_VERSION`].
    Corrupt(&'static str),
}

impl std::fmt::Display for PersistError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PersistError::Io(e) => write!(f, "failed to access the oca data file: {e}"),
            PersistError::Encode(e) => write!(f, "failed to serialize oca data: {e}"),
            PersistError::Decode(e) => write!(f, "failed to parse oca data: {e}"),
            PersistError::Project(e) => write!(f, "failed to read or write the project: {e}"),
            PersistError::Corrupt(msg) => write!(f, "not a valid oca binary file: {msg}"),
        }
    }
}

impl std::error::Error for PersistError {}

/// Shared encoder behind the project/prefs and queue framings.
fn to_framed_bytes<T: Serialize>(value: &T, magic: [u8; 4]) -> Result<Vec<u8>, PersistError> {
    let mut msgpack = Vec::new();
    value
        .serialize(&mut rmp_serde::Serializer::new(&mut msgpack).with_struct_map())
        .map_err(PersistError::Encode)?;

    let mut out = Vec::with_capacity(msgpack.len() / 2 + 5);
    out.extend_from_slice(&magic);
    out.push(FORMAT_VERSION);
    let mut encoder = GzEncoder::new(&mut out, Compression::fast());
    encoder.write_all(&msgpack).map_err(PersistError::Io)?;
    encoder.finish().map_err(PersistError::Io)?;
    Ok(out)
}

/// Shared decoder behind the project/prefs and queue framings.
fn from_framed_bytes<T: DeserializeOwned>(
    bytes: &[u8],
    expected_magic: [u8; 4],
) -> Result<T, PersistError> {
    if bytes.len() < expected_magic.len() + 1 || bytes[..expected_magic.len()] != expected_magic {
        return Err(PersistError::Corrupt("bad magic bytes"));
    }
    let version = bytes[expected_magic.len()];
    if version != FORMAT_VERSION {
        return Err(PersistError::Corrupt("unsupported format version"));
    }
    let mut msgpack = Vec::new();
    // Read one byte past the cap so an exactly-at-the-limit stream still succeeds while
    // anything larger is caught here, before it's ever handed to the MessagePack decoder.
    let decoder = GzDecoder::new(&bytes[expected_magic.len() + 1..]);
    decoder
        .take(MAX_DECOMPRESSED_BYTES + 1)
        .read_to_end(&mut msgpack)
        .map_err(PersistError::Io)?;
    if msgpack.len() as u64 > MAX_DECOMPRESSED_BYTES {
        return Err(PersistError::Corrupt("decompressed data too large"));
    }
    rmp_serde::from_slice(&msgpack).map_err(PersistError::Decode)
}

/// Serializes `value` to the `.ocproj`/`prefs.oc` framing (`OCPJ` + version + compressed
/// MessagePack).
pub fn to_ocproj_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>, PersistError> {
    to_framed_bytes(value, *PROJECT_MAGIC)
}

/// Parses a value previously produced by [`to_ocproj_bytes`].
pub fn from_ocproj_bytes<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, PersistError> {
    from_framed_bytes(bytes, *PROJECT_MAGIC)
}

/// Serializes `value` to the `.ocqueue` framing (`OCQU` + version + compressed MessagePack).
pub fn to_ocqueue_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>, PersistError> {
    to_framed_bytes(value, *QUEUE_MAGIC)
}

/// Parses a value previously produced by [`to_ocqueue_bytes`]. Project/prefs files are rejected
/// by their different magic bytes before MessagePack decoding begins.
pub fn from_ocqueue_bytes<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, PersistError> {
    from_framed_bytes(bytes, *QUEUE_MAGIC)
}

/// Same framing (`OCTR` + version + compressed MessagePack) for a transcript-document sidecar
/// (`crate::transcript::TranscriptDocument`, CF-01) — a wrong-file/corrupt-file error is
/// deterministic before deserialization even starts, same as project/queue files.
pub fn to_octr_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>, PersistError> {
    to_framed_bytes(value, *TRANSCRIPT_MAGIC)
}

pub fn from_octr_bytes<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, PersistError> {
    from_framed_bytes(bytes, *TRANSCRIPT_MAGIC)
}

/// Same framing (`OCSI` + version + compressed MessagePack) for a semantic-search index sidecar
/// (`crate::semantic_index::SemanticIndex`, CF-08) — embeddings are opaque float vectors, not
/// human-inspectable content, so this reuses the compact binary `.ocproj`-style framing rather
/// than `motion_template`'s plain-JSON choice (that format is deliberately shareable/inspectable
/// data; this one is internal cache state, closer in spirit to `.ocqueue`/`.octr`).
pub fn to_ocsi_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>, PersistError> {
    to_framed_bytes(value, *SEMANTIC_INDEX_MAGIC)
}

pub fn from_ocsi_bytes<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, PersistError> {
    from_framed_bytes(bytes, *SEMANTIC_INDEX_MAGIC)
}

/// Writes `bytes` to `path` so a crash or a full disk cannot leave a half-written file: the data
/// goes to a sibling temporary file first, is flushed to disk, and only then replaces `path`. A
/// failed attempt leaves the previous file untouched and removes the temporary one.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut name = path
        .file_name()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "no file name"))?
        .to_os_string();
    // Unique per call, so two writers of the same file never share (and delete) one another's
    // temporary file.
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    name.push(format!(
        ".tmp-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let temporary = path.with_file_name(name);
    let written = fs::File::create(&temporary).and_then(|mut file| {
        file.write_all(bytes)?;
        file.sync_all()
    });
    let replaced = written.and_then(|()| fs::rename(&temporary, path));
    if replaced.is_err() {
        let _ = fs::remove_file(&temporary);
    } else {
        sync_parent_dir(path);
    }
    replaced
}

/// Best-effort `fsync` of the directory holding `path`, so the rename itself survives a power
/// cut. Not possible on Windows, where the replace is already durable enough.
fn sync_parent_dir(path: &Path) {
    #[cfg(unix)]
    if let Some(dir) = path.parent() {
        let dir = if dir.as_os_str().is_empty() {
            Path::new(".")
        } else {
            dir
        };
        if let Ok(dir) = fs::File::open(dir) {
            let _ = dir.sync_all();
        }
    }
    #[cfg(not(unix))]
    let _ = path;
}

/// Writes `project` to `path` as `.ocproj`, overwriting any existing file. The write is atomic
/// (see [`write_atomic`]), so a failure never costs the previously saved copy.
pub fn save_project_to_file(project: &Project, path: &Path) -> Result<(), PersistError> {
    let bytes = to_project_bytes(project)?;
    write_atomic(path, &bytes).map_err(PersistError::Io)
}

/// Reads and parses a project from `path`. The returned [`Project::file_path`] is `None` —
/// callers that want it populated (so subsequent saves go back to the same file) should set
/// it themselves, since only the caller knows whether `path` should be remembered.
pub fn load_project_from_file(path: &Path) -> Result<Project, PersistError> {
    let bytes = fs::read(path).map_err(PersistError::Io)?;
    from_project_bytes(&bytes)
}

/// Serializes `project` to the project file layout (see [`PROJECT_FORMAT_VERSION`]).
pub fn to_project_bytes(project: &Project) -> Result<Vec<u8>, PersistError> {
    let payload = postcard::to_stdvec(project).map_err(PersistError::Project)?;
    let size =
        u32::try_from(payload.len()).map_err(|_| PersistError::Corrupt("project too large"))?;
    let compressed = lz4_flex::block::compress(&payload);
    let mut out = Vec::with_capacity(compressed.len() + PROJECT_MAGIC.len() + 5);
    out.extend_from_slice(PROJECT_MAGIC);
    out.push(PROJECT_FORMAT_VERSION);
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(&compressed);
    Ok(out)
}

/// Parses bytes produced by [`to_project_bytes`].
pub fn from_project_bytes(bytes: &[u8]) -> Result<Project, PersistError> {
    let header = PROJECT_MAGIC.len();
    if bytes.len() < header + 1 || &bytes[..header] != PROJECT_MAGIC {
        return Err(PersistError::Corrupt("bad magic bytes"));
    }
    let version = bytes[header];
    if version != PROJECT_FORMAT_VERSION && version != PROJECT_FORMAT_VERSION_V3 {
        return Err(PersistError::Corrupt("unsupported format version"));
    }
    let Some((size, compressed)) = bytes[header + 1..].split_first_chunk::<4>() else {
        return Err(PersistError::Corrupt("truncated header"));
    };
    // The size comes from the file, so it is capped before anything is allocated for it.
    let size = u64::from(u32::from_le_bytes(*size));
    if size > MAX_DECOMPRESSED_BYTES {
        return Err(PersistError::Corrupt("decompressed data too large"));
    }
    let payload = lz4_flex::block::decompress(compressed, size as usize)
        .map_err(|_| PersistError::Corrupt("damaged compressed data"))?;
    let payload = if version == PROJECT_FORMAT_VERSION_V3 {
        upgrade_v3(payload)
    } else {
        payload
    };
    postcard::from_bytes(&payload).map_err(PersistError::Project)
}

/// Turns a version-3 payload into a version-4 one. `Project::frame_rate` was added as the last
/// field, so an older file is a complete current file minus that trailing `None`.
fn upgrade_v3(mut payload: Vec<u8>) -> Vec<u8> {
    payload.push(POSTCARD_NONE);
    payload
}
