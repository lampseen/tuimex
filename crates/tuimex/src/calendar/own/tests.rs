use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use super::*;

fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-scratch")
        .join(format!("own-{}-{name}", std::process::id()));
    if dir.exists() {
        std::fs::remove_dir_all(&dir).unwrap();
    }
    dir
}

#[test]
fn what_was_ignored_and_proposed_is_kept_across_runs_in_a_file_only_its_owner_reads() {
    let data = scratch("kept");
    assert_eq!(Own::load(&data), Ok(Own::default()), "nothing was ever kept");
    let mut own = Own::default();
    own.ignored.push(("work".into(), "lunch\twith a tab\n@example.com".into()));
    own.proposed.push(("work".into(), "planning@example.com".into(), 1_796_115_600));
    own.proposed.push(("home".into(), "planning@example.com".into(), 5));
    own.save(&data).unwrap();
    own.save(&data).unwrap();

    assert_eq!(Own::load(&data), Ok(own.clone()), "a UID is hostile text, and comes back as it was");
    let file = data.join(LOCAL).join(FILE);
    assert_eq!(std::fs::metadata(&file).unwrap().permissions().mode() & 0o777, 0o600);
    assert!(own.ignores("work", "lunch\twith a tab\n@example.com") && !own.ignores("home", "planning@example.com"));

    own.settle("work", "planning@example.com");
    assert_eq!(own.proposed, [("home".to_string(), "planning@example.com".to_string(), 5)]);
}

#[test]
fn a_file_that_does_not_read_is_said_and_not_started_over() {
    let data = scratch("unreadable");
    std::fs::create_dir_all(data.join(LOCAL)).unwrap();
    std::fs::write(data.join(LOCAL).join(FILE), "not json").unwrap();
    let error = Own::load(&data).unwrap_err();
    assert!(error.contains(FILE), "{error}");
    // No event's file could have this name, so the store lists none for it.
    let store = tuimex_store::calendar::Calendars::open(&data, &data.join("cache")).unwrap();
    assert!(store.held().unwrap().is_empty());
}
