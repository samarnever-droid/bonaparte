//! The disk journal contract at session level: deep sessions undo to the
//! bottom, the journal file is real on disk, and opening a project starts a
//! clean history. One temp dir per test binary, serialised through a mutex
//! because the journal location is an environment override.

use bonaparte_model::{HistoryJournal, JournalEntry, Op, HISTORY_WINDOW};
use bonaparte_runtime::journal::FileJournal;
use serde_json::json;
use std::sync::{Mutex, OnceLock};

fn temp_history_dir() -> &'static std::path::PathBuf {
    static DIR: OnceLock<std::path::PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        let dir = std::env::temp_dir().join(format!("bonaparte-journal-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("BONAPARTE_HISTORY_DIR", &dir);
        dir
    })
}

/// The env override is process-global; keep journal tests serial so the
/// session test can never race the file test for the directory.
fn lock() -> std::sync::MutexGuard<'static, ()> {
    static SERIALIZED: Mutex<()> = Mutex::new(());
    SERIALIZED.lock().unwrap_or_else(|p| p.into_inner())
}

fn entry(i: i64) -> JournalEntry {
    JournalEntry {
        op: Op::RenameProject {
            name: format!("v{i}"),
        },
        label: format!("Renamed project to “v{i}”"),
        edit_group: None,
    }
}

#[test]
fn file_journal_round_trips_appends_peeks_and_truncates() {
    let _guard = lock();
    let dir = temp_history_dir();
    let journal_path = dir.join(format!("undo-roundtrip-{}.jsonl", std::process::id()));
    let mut journal = {
        // Open through the public constructor, then exercise it like History
        // does: append forward, take back newest-first, peek without losing.
        FileJournal::open_for_path(&journal_path).unwrap()
    };
    for i in 0..500 {
        journal.append(&entry(i)).unwrap();
    }
    assert_eq!(journal.len(), 500);
    let on_disk = std::fs::metadata(&journal_path).unwrap().len();
    assert!(on_disk > 10_000, "500 labelled ops should be a real file");
    assert_eq!(
        journal.peek_last().unwrap().label,
        "Renamed project to “v499”"
    );
    for i in (0..500).rev() {
        assert_eq!(journal.take_last().unwrap().label, entry(i).label);
    }
    assert!(journal.take_last().is_none(), "empty must stay empty");
    assert_eq!(
        std::fs::metadata(&journal_path).unwrap().len(),
        0,
        "truncated back"
    );
    // Re-append after truncation: offsets restart cleanly, no sparse tail.
    journal.append(&entry(999)).unwrap();
    assert_eq!(journal.take_last().unwrap().label, entry(999).label);
    journal.append(&entry(1)).unwrap();
    journal.append(&entry(2)).unwrap();
    journal.clear();
    assert_eq!(journal.len(), 0);
    assert_eq!(std::fs::metadata(&journal_path).unwrap().len(), 0);
    let _ = std::fs::remove_file(&journal_path);
}

#[test]
fn a_journaled_history_walks_to_the_bottom_and_back_through_files() {
    let _guard = lock();
    let dir = temp_history_dir().join(format!("session-{}", std::process::id()));
    std::env::set_var("BONAPARTE_HISTORY_DIR", &dir);

    let mut session = bonaparte_runtime::EditorSession::default();
    for i in 0..1250 {
        session
            .command(
                "apply",
                json!({"op": { "type": "renameProject", "name": format!("flood {i}") }}),
            )
            .unwrap();
    }
    let state = session.command("state", json!({})).unwrap();
    assert_eq!(state["historyDepth"], json!(1250), "full session depth");
    assert_eq!(state["historyOverflow"], json!(250), "the spill is real");
    assert_eq!(
        state["history"].as_array().unwrap().len(),
        HISTORY_WINDOW,
        "the UI list stays window-sized"
    );

    // Undo to the absolute bottom through the journal file, then redo home.
    for _ in 0..1250 {
        session.command("undo", json!({})).unwrap();
    }
    let state = session.command("state", json!({})).unwrap();
    assert_eq!(
        state["project"]["name"],
        json!("Orbit — Studio ident"),
        "undo past the window must still be exact"
    );
    assert_eq!(state["canUndo"], json!(false));
    for _ in 0..1250 {
        session.command("redo", json!({})).unwrap();
    }
    let state = session.command("state", json!({})).unwrap();
    assert_eq!(state["project"]["name"], json!("flood 1249"));

    // A saved file, opened fresh, starts with clean (empty) history.
    let current = session.command("state", json!({})).unwrap()["project"].clone();
    let text = serde_json::to_string(&current).unwrap();
    session
        .command("open_project", json!({ "json": text }))
        .unwrap();
    let state = session.command("state", json!({})).unwrap();
    assert_eq!(state["historyDepth"], json!(0), "opening is a new session");
    assert!(
        dir.join(format!("undo-{}.jsonl", std::process::id()))
            .exists(),
        "the session journal file lives in the configured directory"
    );
    assert_eq!(
        std::fs::metadata(dir.join(format!("undo-{}.jsonl", std::process::id())))
            .unwrap()
            .len(),
        0,
        "and truncates clean for the new project"
    );
    std::env::set_var("BONAPARTE_HISTORY_DIR", dir.parent().unwrap());
    let _ = std::fs::remove_dir_all(&dir);
}
