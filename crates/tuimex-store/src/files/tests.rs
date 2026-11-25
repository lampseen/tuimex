use std::fs;
use std::io::{ErrorKind, Write};
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::PathBuf;

use super::{append, create, place, private_dir, read_own, replace};

/// A directory of this test's own under the build directory. Never the
/// system temporary directory, and never anything the user owns.
fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-scratch")
        .join(format!("store-files-{}-{name}", std::process::id()));
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    dir
}

fn mode(path: &std::path::Path) -> u32 {
    fs::metadata(path).unwrap().permissions().mode() & 0o777
}

#[test]
fn a_directory_and_its_missing_parents_are_owner_only() {
    let root = scratch("dir");
    private_dir(&root.join("a/b")).unwrap();
    assert_eq!(mode(&root.join("a/b")), 0o700);
    assert_eq!(mode(&root.join("a")), 0o700);
    assert_eq!(mode(&root), 0o700);
}

#[test]
fn a_directory_that_was_wider_is_narrowed() {
    let root = scratch("wide");
    fs::create_dir_all(&root).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o755)).unwrap();
    private_dir(&root).unwrap();
    assert_eq!(mode(&root), 0o700);
}

#[test]
fn a_new_file_is_owner_only() {
    let root = scratch("file");
    private_dir(&root).unwrap();
    let path = root.join("f");
    create(&path).unwrap().write_all(b"x").unwrap();
    assert_eq!(mode(&path), 0o600);
}

#[test]
fn create_refuses_a_path_that_exists() {
    let root = scratch("exists");
    private_dir(&root).unwrap();
    let path = root.join("f");
    create(&path).unwrap();
    assert_eq!(create(&path).unwrap_err().kind(), ErrorKind::AlreadyExists);
}

#[test]
fn create_does_not_write_through_a_planted_link() {
    let root = scratch("link");
    private_dir(&root).unwrap();
    let victim = root.join("victim");
    fs::write(&victim, "untouched").unwrap();
    symlink(&victim, root.join("f")).unwrap();
    assert!(create(&root.join("f")).is_err());
    assert_eq!(fs::read_to_string(&victim).unwrap(), "untouched");
}

#[test]
fn append_keeps_what_an_earlier_run_wrote() {
    let root = scratch("append");
    private_dir(&root).unwrap();
    let path = root.join("log");
    append(&path).unwrap().write_all(b"one\n").unwrap();
    append(&path).unwrap().write_all(b"two\n").unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), "one\ntwo\n");
    assert_eq!(mode(&path), 0o600);
}

#[test]
fn append_narrows_a_file_that_was_wider() {
    let root = scratch("append-wide");
    private_dir(&root).unwrap();
    let path = root.join("log");
    fs::write(&path, "").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    append(&path).unwrap();
    assert_eq!(mode(&path), 0o600);
}

#[test]
fn append_does_not_write_through_a_planted_link() {
    let root = scratch("append-link");
    private_dir(&root).unwrap();
    let victim = root.join("victim");
    fs::write(&victim, "untouched").unwrap();
    symlink(&victim, root.join("log")).unwrap();
    assert!(append(&root.join("log")).is_err());
    assert_eq!(fs::read_to_string(&victim).unwrap(), "untouched");
}

#[test]
fn place_moves_a_file_and_refuses_a_name_that_is_taken() {
    let root = scratch("place");
    private_dir(&root).unwrap();
    create(&root.join("a")).unwrap().write_all(b"a").unwrap();
    place(&root.join("a"), &root.join("b")).unwrap();
    assert!(!root.join("a").exists());
    assert_eq!(fs::read_to_string(root.join("b")).unwrap(), "a");
    assert_eq!(mode(&root.join("b")), 0o600);

    create(&root.join("c")).unwrap().write_all(b"c").unwrap();
    assert_eq!(place(&root.join("c"), &root.join("b")).unwrap_err().kind(), ErrorKind::AlreadyExists);
    assert_eq!(fs::read_to_string(root.join("b")).unwrap(), "a");
    assert!(root.join("c").exists(), "a refused file stays where it was");
}

#[test]
fn replace_writes_a_new_file_and_then_swaps_its_content() {
    let root = scratch("replace");
    private_dir(&root).unwrap();
    let path = root.join("f");
    replace(&path, b"one").unwrap();
    replace(&path, b"two").unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), "two");
    assert_eq!(mode(&path), 0o600);
    assert!(!root.join("f.tmp").exists());
}

#[test]
fn replace_survives_what_a_dead_run_left_and_does_not_write_through_a_link() {
    let root = scratch("replace-stale");
    private_dir(&root).unwrap();
    let victim = root.join("victim");
    fs::write(&victim, "untouched").unwrap();
    symlink(&victim, root.join("f.tmp")).unwrap();
    replace(&root.join("f"), b"new").unwrap();
    assert_eq!(fs::read_to_string(root.join("f")).unwrap(), "new");
    assert_eq!(fs::read_to_string(&victim).unwrap(), "untouched");
}

#[test]
fn replace_puts_a_file_where_a_link_was_and_leaves_its_target_alone() {
    let root = scratch("replace-link");
    private_dir(&root).unwrap();
    let victim = root.join("victim");
    fs::write(&victim, "untouched").unwrap();
    symlink(&victim, root.join("f")).unwrap();
    replace(&root.join("f"), b"new").unwrap();
    assert!(fs::symlink_metadata(root.join("f")).unwrap().file_type().is_file());
    assert_eq!(fs::read_to_string(root.join("f")).unwrap(), "new");
    assert_eq!(fs::read_to_string(&victim).unwrap(), "untouched");
}

#[test]
fn read_own_reads_a_file_up_to_a_bound_and_refuses_a_link_or_a_directory() {
    let root = scratch("read-own");
    private_dir(&root).unwrap();
    replace(&root.join("f"), b"on\n").unwrap();
    assert_eq!(read_own(&root.join("f"), 64).unwrap(), b"on\n");
    assert_eq!(read_own(&root.join("f"), 1).unwrap(), b"o");
    symlink(root.join("f"), root.join("link")).unwrap();
    assert_eq!(read_own(&root.join("link"), 64).unwrap_err().kind(), ErrorKind::InvalidInput);
    assert_eq!(read_own(&root, 64).unwrap_err().kind(), ErrorKind::InvalidInput);
    assert_eq!(read_own(&root.join("absent"), 64).unwrap_err().kind(), ErrorKind::NotFound);
}
