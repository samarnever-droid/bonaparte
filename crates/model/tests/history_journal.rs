//! The disk-journal contract: entries that leave the hot window spill to the
//! journal and stay undoable for the whole session — full depth on disk,
//! window-sized memory. `HistoryJournal` is the sink; the runtime's file
//! backend and this in-memory one are tested against the same expectations.

use bonaparte_model::*;

fn fixture() -> (Project, CompId, LayerId) {
    let mut project = Project::new("Journal");
    let comp = project.create_comp("Main", 32, 32, FrameRate::FPS_30, Time(480_000));
    let layer = project.insert_layer(
        comp,
        Layer::new_solid("Solid", [0.25, 0.4, 0.6, 1.0], Time::ZERO, Time(480_000)),
    );
    (project, comp, layer)
}

fn set_rotation(value: f32) -> Op {
    Op::SetValue {
        comp: CompId(1),
        layer: LayerId(1),
        property: Property::Rotation,
        value: PropValue::Scalar(value),
    }
}

fn journalled() -> History {
    History::with_journals(
        Box::<MemoryJournal>::default(),
        Box::<MemoryJournal>::default(),
    )
}

#[test]
fn overflow_spills_to_the_journal_and_full_history_stays_undoable() {
    let (mut project, _, _) = fixture();
    let pristine = serde_json::to_value(&project).unwrap();
    let mut history = journalled();

    for i in 0..(HISTORY_WINDOW + 250) {
        history
            .commit(&mut project, set_rotation(i as f32))
            .unwrap();
    }
    // Memory holds the window; the rest lives on the sink — total depth is
    // the whole session.
    assert_eq!(history.undo_window_len(), HISTORY_WINDOW);
    assert_eq!(history.undo_overflow(), 250);
    assert_eq!(history.undo_len(), HISTORY_WINDOW + 250);

    // Undo everything, crossing from window into journal without a seam.
    for _ in 0..(HISTORY_WINDOW + 250) {
        assert!(history.undo(&mut project).unwrap());
    }
    assert!(!history.undo(&mut project).unwrap(), "bottom must be exact");
    assert_eq!(serde_json::to_value(&project).unwrap(), pristine);

    // Redo all the way back — the spilled forward entries come home too.
    for _ in 0..(HISTORY_WINDOW + 250) {
        assert!(history.redo(&mut project).unwrap());
    }
    assert!(!history.redo(&mut project).unwrap(), "top must be exact");
    assert_eq!(
        history.undo_window_len() + history.undo_overflow(),
        HISTORY_WINDOW + 250
    );
}

#[test]
fn deep_undo_spills_redo_past_the_window_to_the_redo_journal() {
    let (mut project, _, _) = fixture();
    let mut history = journalled();
    for i in 0..(HISTORY_WINDOW + 100) {
        history
            .commit(&mut project, set_rotation(i as f32))
            .unwrap();
    }
    // Undo everything: forward entries fill the redo window, then spill.
    while history.undo(&mut project).unwrap() {}
    assert_eq!(history.redo_window_len(), HISTORY_WINDOW);
    assert_eq!(history.redo_overflow(), 100);
    assert_eq!(history.undo_len(), 0);
    assert!(history.can_redo());
    while history.redo(&mut project).unwrap() {}
    assert_eq!(history.undo_len(), HISTORY_WINDOW + 100);
}

#[test]
fn a_new_commit_after_undone_history_clears_both_redo_stacks() {
    let (mut project, _, _) = fixture();
    let mut history = journalled();
    for i in 0..(HISTORY_WINDOW + 60) {
        history
            .commit(&mut project, set_rotation(i as f32))
            .unwrap();
    }
    while history.undo(&mut project).unwrap() {}
    assert!(history.redo_overflow() > 0, "prerequisite: redo spilled");
    history.commit(&mut project, set_rotation(99.0)).unwrap();
    assert_eq!(history.redo_len(), 0, "forked redo must vanish entirely");
    assert_eq!(history.redo_overflow(), 0);
}

#[test]
fn labels_survive_the_spill_peek() {
    let (mut project, _, _) = fixture();
    let mut history = journalled();
    for i in 0..(HISTORY_WINDOW + 3) {
        history
            .commit(&mut project, set_rotation(i as f32))
            .unwrap();
    }
    while history.undo(&mut project).unwrap() {}
    // Redo-top now lives in the redo journal: the label must still be named.
    let label = history
        .redo_top_label()
        .expect("journal-backed peek must name the next redo");
    assert!(label.contains("Rotation"), "unexpected label: {label}");
    assert!(history.redo(&mut project).unwrap());
}

#[test]
fn without_a_journal_the_window_evicts_exactly_like_before() {
    let (mut project, _, _) = fixture();
    let mut history = History::new();
    for i in 0..(HISTORY_WINDOW + 50) {
        history
            .commit(&mut project, set_rotation(i as f32))
            .unwrap();
    }
    assert_eq!(history.undo_len(), HISTORY_WINDOW, "no journal, no depth");
    assert_eq!(history.undo_overflow(), 0);
    let mut steps = 0;
    while history.undo(&mut project).unwrap() {
        steps += 1;
    }
    assert_eq!(steps, HISTORY_WINDOW);
}

#[test]
fn edit_group_merging_is_unaffected_by_the_journal() {
    let (mut project, _, _) = fixture();
    let mut history = journalled();
    for i in 0..300 {
        history
            .commit_grouped(
                &mut project,
                set_rotation(i as f32),
                Some("live-drag".into()),
            )
            .unwrap();
    }
    // 300 grouped edits merge into ONE transaction, nowhere near a spill.
    assert_eq!(history.undo_len(), 1);
    assert_eq!(history.undo_overflow(), 0);
}
