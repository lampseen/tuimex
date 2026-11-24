use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::{Dirs, scratch_root};

fn resolve(vars: &[(&str, &str)]) -> Dirs {
    let m: HashMap<&str, PathBuf> = vars.iter().map(|(k, v)| (*k, PathBuf::from(v))).collect();
    Dirs::resolve(|k| m.get(k).cloned(), Path::new("/h"))
}

#[test]
fn falls_back_to_home_when_xdg_is_unset() {
    let d = resolve(&[]);
    assert_eq!(d.config, Path::new("/h/.config/tuimex"));
    assert_eq!(d.data, Path::new("/h/.local/share/tuimex"));
    assert_eq!(d.cache, Path::new("/h/.cache/tuimex"));
    assert_eq!(d.state, Path::new("/h/.local/state/tuimex"));
}

#[test]
fn honours_xdg_variables() {
    let d = resolve(&[
        ("XDG_CONFIG_HOME", "/c"),
        ("XDG_DATA_HOME", "/d"),
        ("XDG_CACHE_HOME", "/k"),
        ("XDG_STATE_HOME", "/s"),
    ]);
    assert_eq!(d.config, Path::new("/c/tuimex"));
    assert_eq!(d.data, Path::new("/d/tuimex"));
    assert_eq!(d.cache, Path::new("/k/tuimex"));
    assert_eq!(d.state, Path::new("/s/tuimex"));
}

#[test]
fn a_relative_xdg_value_is_ignored_not_joined_to_the_current_directory() {
    let d = resolve(&[("XDG_CONFIG_HOME", "rel/config"), ("XDG_STATE_HOME", ".")]);
    assert_eq!(d.config, Path::new("/h/.config/tuimex"));
    assert_eq!(d.state, Path::new("/h/.local/state/tuimex"));
}

#[test]
fn the_four_directories_are_distinct() {
    let d = resolve(&[]);
    let all = [&d.config, &d.data, &d.cache, &d.state];
    for (i, a) in all.iter().enumerate() {
        assert!(all[i + 1..].iter().all(|b| a != b), "{} is used twice", a.display());
    }
}

#[test]
fn a_unit_test_never_sees_the_real_directories() {
    let d = Dirs::from_env().unwrap();
    for dir in [&d.config, &d.data, &d.cache, &d.state] {
        assert!(dir.starts_with(scratch_root()), "{} is outside the scratch root", dir.display());
    }
}
