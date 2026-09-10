//! File-backed [`HistoryJournal`] — the disk half of "the whole session
//! stays undoable". The editor keeps a hot in-memory window; every entry
//! that leaves it appends as one JSON line (identical shape to the wire
//! `Op`s) to `~/.bonaparte/history/` (override with `BONAPARTE_HISTORY_DIR`),
//! so a marathon session can undo to frame zero without RAM growing.
//!
//! The reader is a separate file handle with its own cursor: truncation and
//! append only ever touch bytes the reader has already been asked for, and
//! `peek_last` (a `&self` contract) takes it through a `Mutex`, which also
//! makes the whole journal `Send + Sync` — bridge workers may touch a shared
//! session's history from any thread.
use bonaparte_model::{HistoryJournal, JournalEntry};
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::sync::Mutex;

/// One history transaction per line: `{"op":…,"label":…,"editGroup":…}`.
#[derive(Debug)]
pub struct FileJournal {
    path: PathBuf,
    writer: File,
    reader: Mutex<File>,
    /// Byte offset of every record, in append order. 8 bytes per spilled
    /// entry is the whole in-memory cost of infinite history.
    offsets: Vec<u64>,
    bytes: u64,
}

impl FileJournal {
    /// Create (truncating) the journal for this process. `kind` separates
    /// the undo and redo sinks: `undo-<pid>.jsonl` / `redo-<pid>.jsonl`.
    /// Every new session reopens the same file and truncates it — opening a
    /// project starts a clean history, exactly like the in-memory stacks.
    pub fn open(kind: &str) -> Result<Self, String> {
        let dir = history_dir();
        std::fs::create_dir_all(&dir).map_err(|e| format!("history dir unavailable: {e}"))?;
        sweep_stale(&dir);
        let path = dir.join(format!("{kind}-{}.jsonl", std::process::id()));
        let writer = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&path)
            .map_err(|e| format!("history file unavailable: {e}"))?;
        let reader = OpenOptions::new()
            .read(true)
            .open(&path)
            .map_err(|e| format!("history file unreadable: {e}"))?;
        Ok(Self {
            path,
            writer,
            reader: Mutex::new(reader),
            offsets: Vec::new(),
            bytes: 0,
        })
    }

    /// Open a journal at an exact path (tests, embedders, per-project files).
    pub fn open_for_path(path: &std::path::Path) -> Result<Self, String> {
        let writer = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)
            .map_err(|e| format!("history file unavailable: {e}"))?;
        let reader = OpenOptions::new()
            .read(true)
            .open(path)
            .map_err(|e| format!("history file unreadable: {e}"))?;
        Ok(Self {
            path: path.to_path_buf(),
            writer,
            reader: Mutex::new(reader),
            offsets: Vec::new(),
            bytes: 0,
        })
    }

    /// Test/inspection helper.
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    fn read_span(&self, start: u64, end: u64) -> std::io::Result<Vec<u8>> {
        let mut reader = self
            .reader
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        reader.seek(SeekFrom::Start(start))?;
        let mut buffer = vec![0u8; (end - start) as usize];
        reader.read_exact(&mut buffer)?;
        Ok(buffer)
    }
}

impl HistoryJournal for FileJournal {
    fn append(&mut self, entry: &JournalEntry) -> Result<(), String> {
        let mut line = serde_json::to_vec(entry).map_err(|e| format!("journal serialise: {e}"))?;
        line.push(b'\n');
        self.offsets.push(self.bytes);
        self.bytes += line.len() as u64;
        self.writer
            .seek(SeekFrom::End(0))
            .and_then(|_| self.writer.write_all(&line))
            .and_then(|_| self.writer.flush())
            .map_err(|e| {
                self.offsets.pop();
                self.bytes -= line.len() as u64;
                format!("journal write failed: {e}")
            })
    }

    fn take_last(&mut self) -> Option<JournalEntry> {
        let start = *self.offsets.last()?;
        let raw = self.read_span(start, self.bytes).ok()?;
        // Truncate first, parse second: a corrupt tail must not let the same
        // bytes poison every later undo — history shortens instead.
        self.bytes = start;
        self.offsets.pop();
        if self
            .writer
            .seek(SeekFrom::Start(start))
            .and_then(|_| self.writer.set_len(start))
            .is_err()
        {
            return None;
        }
        serde_json::from_slice(&raw).ok()
    }

    fn peek_last(&self) -> Option<JournalEntry> {
        let start = *self.offsets.last()?;
        let raw = self.read_span(start, self.bytes).ok()?;
        serde_json::from_slice(&raw).ok()
    }

    fn len(&self) -> usize {
        self.offsets.len()
    }

    fn clear(&mut self) {
        self.offsets.clear();
        self.bytes = 0;
        let _ = self
            .writer
            .seek(SeekFrom::Start(0))
            .and_then(|_| self.writer.set_len(0))
            .and_then(|_| self.writer.flush());
    }
}

/// `$BONAPARTE_HISTORY_DIR`, else the shared per-user shelf beside the Vault.
pub fn history_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("BONAPARTE_HISTORY_DIR") {
        return PathBuf::from(dir);
    }
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".bonaparte").join("history")
}

/// Best-effort housekeeping: pid-scoped files belong to live processes, so
/// anything older than a fortnight is residue from a dead one. Never fails.
fn sweep_stale(dir: &std::path::Path) {
    let Ok(read_dir) = std::fs::read_dir(dir) else {
        return;
    };
    let cutoff = std::time::SystemTime::now() - std::time::Duration::from_secs(14 * 86_400);
    for entry in read_dir.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy().to_string();
        let mine = name.ends_with(&format!("-{}.jsonl", std::process::id()));
        if mine || !name.ends_with(".jsonl") {
            continue;
        }
        let fresh = entry
            .metadata()
            .and_then(|m| m.modified())
            .map(|t| t > cutoff)
            .unwrap_or(true);
        if !fresh {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// Journal-backed history for a fresh [`crate::EditorSession`]; falls back to
/// the pure in-memory window when the filesystem refuses — a read-only home
/// directory may not cost the user an editing session.
pub fn fresh_history() -> bonaparte_model::History {
    match (FileJournal::open("undo"), FileJournal::open("redo")) {
        (Ok(undo), Ok(redo)) => {
            bonaparte_model::History::with_journals(Box::new(undo), Box::new(redo))
        }
        _ => bonaparte_model::History::new(),
    }
}
