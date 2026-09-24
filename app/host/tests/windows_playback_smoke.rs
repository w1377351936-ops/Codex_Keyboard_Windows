#![cfg(windows)]

use easy_codex_host::cache::CacheId;
use easy_codex_host::store::{StateStore, SummaryClaimResult};
use rusqlite::{Connection, params};

#[test]
fn interrupted_playback_preserves_unread_and_rejects_old_completion() {
    const TASK: &str = "019fa972-5cfa-75e1-9008-0b17ade9a347";
    const TURN: &str = "019fa972-5cfa-75e1-9008-0b17ade9a348";
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("state.sqlite3");
    let mut store = StateStore::open(&path).unwrap();
    store.set_binding(3, None, TASK).unwrap().unwrap();
    Connection::open(&path)
        .unwrap()
        .execute(
            "INSERT INTO completion_ledger
             (completion_id, task_id, rollout_cursor, observed_at, turn_pack)
             VALUES (?1, ?2, 'fixture', unixepoch(), ?3)",
            params![TURN, TASK, r#"{"turn":1}"#],
        )
        .unwrap();
    let SummaryClaimResult::Claimed(claim) = store
        .claim_summary(TASK, "request-1")
        .unwrap()
        .unwrap()
    else {
        panic!("expected summary claim");
    };
    let cache = CacheId::for_task(TASK, 1).unwrap().reference();
    store.publish_summary(&claim, &cache).unwrap();

    let old_lease = store.acquire_summary_playback(3, 11, 19, 41).unwrap().unwrap();
    drop(store);

    let mut restarted = StateStore::open(&path).unwrap();
    assert_eq!(restarted.current_unread_summary(TASK).unwrap().unwrap().generation, 1);
    assert!(!restarted.finish_summary_playback(&old_lease).unwrap());

    let current_lease = restarted.acquire_summary_playback(3, 12, 20, 42).unwrap().unwrap();
    assert!(!restarted.finish_summary_playback(&old_lease).unwrap());
    assert!(restarted.finish_summary_playback(&current_lease).unwrap());
    assert!(restarted.current_unread_summary(TASK).unwrap().is_none());
}
