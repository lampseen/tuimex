//! Claims the documents make, checked against the things they describe.

// A test that cannot read its inputs should fail loudly, which is what
// unwrap does; clippy's test allowance does not reach helper functions.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read(rel: &str) -> String {
    fs::read_to_string(root().join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

/// Every file under the repository that `keep` accepts. The build
/// directory, the delivered design and dot-directories other than
/// `.github` are not this repository's own text.
fn files(keep: impl Fn(&Path) -> bool) -> Vec<PathBuf> {
    fn walk(dir: &Path, keep: &dyn Fn(&Path) -> bool, out: &mut Vec<PathBuf>) {
        for e in fs::read_dir(dir).unwrap().map(Result::unwrap) {
            let (p, n) = (e.path(), e.file_name());
            let n = n.to_string_lossy();
            if n == "target" || n == "design" || (n.starts_with('.') && n != ".github") {
                continue;
            }
            if p.is_dir() {
                walk(&p, keep, out);
            } else if keep(&p) {
                out.push(p);
            }
        }
    }
    let mut v = Vec::new();
    walk(&root(), &keep, &mut v);
    assert!(!v.is_empty(), "the walk found nothing to check");
    v
}

fn has_ext(p: &Path, exts: &[&str]) -> bool {
    p.extension().is_some_and(|x| exts.iter().any(|e| x == *e))
}

fn makefile_var(name: &str) -> u64 {
    let mk = read("Makefile");
    let line = mk
        .lines()
        .find(|l| l.split_whitespace().next() == Some(name))
        .unwrap_or_else(|| panic!("{name} not in the Makefile"));
    line.rsplit(' ').next().unwrap().trim().parse().unwrap()
}

fn thousands(n: u64) -> String {
    if n >= 1000 { format!("{},{:03}", n / 1000, n % 1000) } else { n.to_string() }
}

#[test]
fn the_documented_budgets_match_the_makefile() {
    let adr = read("docs/decisions.md");
    let bytes = makefile_var("MAX_BINARY_BYTES");
    assert_eq!(bytes % (1024 * 1024), 0, "the binary budget is a whole number of MiB, so a record can quote it");
    let want = [
        format!("{} MiB", bytes / (1024 * 1024)),
        format!("{} lines", thousands(makefile_var("MAX_CODE_LINES"))),
        format!("{}% of the code", makefile_var("MAX_COMMENT_RATIO")),
        format!("{}% of the code", makefile_var("MAX_DOC_RATIO")),
        format!("{} lines", thousands(makefile_var("DOC_RATIO_FROM"))),
    ];
    for w in want {
        assert!(adr.contains(&w), "no record says \"{w}\", which is what the Makefile enforces");
    }
}

#[test]
fn every_relative_doc_link_resolves() {
    let mut broken = Vec::new();
    for md in files(|p| has_ext(p, &["md"])) {
        let text = fs::read_to_string(&md).unwrap();
        let mut rest = text.as_str();
        while let Some(i) = rest.find("](") {
            rest = &rest[i + 2..];
            let Some(end) = rest.find(')') else { break };
            let target = &rest[..end];
            let file = target.split('#').next().unwrap();
            if file.is_empty() || target.contains("://") || target.starts_with("mailto:") {
                continue;
            }
            // A wiki page links to another as `Page-name`, with no
            // extension: the published wiki serves it under that name.
            let in_wiki = md.parent().is_some_and(|d| d.ends_with("wiki"));
            let file = if in_wiki && !file.contains(['.', '/']) { format!("{file}.md") } else { file.to_string() };
            if !md.parent().unwrap().join(file).exists() {
                broken.push(format!("{}: {target}", md.strip_prefix(root()).unwrap().display()));
            }
        }
    }
    assert!(broken.is_empty(), "links to files that do not exist:\n{}", broken.join("\n"));
}

/// Every `ADR-` followed by digits, as written.
fn adr_numbers(text: &str) -> BTreeSet<String> {
    text.match_indices("ADR-")
        .map(|(i, _)| text[i + 4..].chars().take_while(char::is_ascii_digit).collect::<String>())
        .filter(|n| !n.is_empty())
        .collect()
}

/// A record cited from a comment, a document or a workflow is a reason
/// someone will go looking for.
#[test]
fn every_record_cited_exists() {
    let records: BTreeSet<String> = read("docs/decisions.md")
        .lines()
        .filter_map(|l| l.strip_prefix("## ADR-"))
        .map(|l| l.chars().take_while(char::is_ascii_digit).collect())
        .collect();
    assert!(records.len() >= 23, "found only {} records in docs/decisions.md", records.len());
    let mut missing = Vec::new();
    for f in files(|p| has_ext(p, &["md", "rs", "toml", "yml", "py"]) || p.ends_with("Makefile")) {
        for n in adr_numbers(&fs::read_to_string(&f).unwrap()) {
            if !records.contains(&n) {
                missing.push(format!("{}: ADR-{n}", f.strip_prefix(root()).unwrap().display()));
            }
        }
    }
    assert!(missing.is_empty(), "records cited that docs/decisions.md does not have:\n{}", missing.join("\n"));
}

/// CLAUDE.md: unit tests live in a sibling `tests.rs`. The code budget
/// tells tests from code by that filename alone (ADR-004), so an inline
/// `mod tests { … }` would be counted as code and slip past the rule.
#[test]
fn no_test_module_is_inline() {
    let bad: Vec<String> = files(|p| has_ext(p, &["rs"]) && p.components().any(|c| c.as_os_str() == "src"))
        .into_iter()
        .filter(|p| fs::read_to_string(p).unwrap().contains("mod tests {"))
        .map(|p| p.display().to_string())
        .collect();
    assert!(bad.is_empty(), "inline test modules; move them to a sibling tests.rs:\n{}", bad.join("\n"));
}

/// What each crate may name, which is what PLAN §2's arrows reach from it.
/// Naming directly something already reachable adds no capability; an edge
/// outside this table is one the plan's boundaries exist to prevent
/// (ADR-001).
const EDGES: [(&str, &[&str]); 7] = [
    ("tuimex", &["tuimex-ui", "tuimex-engine", "tuimex-store", "tuimex-content", "tuimex-core", "tuimex-fixtures"]),
    ("tuimex-ui", &["tuimex-core"]),
    ("tuimex-engine", &["tuimex-store", "tuimex-content", "tuimex-core"]),
    ("tuimex-store", &["tuimex-core"]),
    ("tuimex-content", &["tuimex-core"]),
    ("tuimex-fixtures", &["tuimex-core"]),
    ("tuimex-core", &[]),
];

fn workspace_deps(manifest: &toml::Table, table: &str) -> Vec<String> {
    let Some(deps) = manifest.get(table) else { return Vec::new() };
    deps.as_table().unwrap().keys().filter(|k| k.starts_with("tuimex")).cloned().collect()
}

#[test]
fn every_crate_edge_is_one_the_plan_allows() {
    let mut found: Vec<String> =
        fs::read_dir(root().join("crates")).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
    found.sort();
    let mut planned: Vec<&str> = EDGES.iter().map(|(name, _)| *name).collect();
    planned.sort();
    assert_eq!(found, planned, "the crates are the seven in PLAN §2");

    let mut bad = Vec::new();
    for (name, allowed) in EDGES {
        let manifest: toml::Table = toml::from_str(&read(&format!("crates/{name}/Cargo.toml"))).unwrap();
        for dep in workspace_deps(&manifest, "dependencies") {
            if !allowed.contains(&dep.as_str()) {
                bad.push(format!("{name} -> {dep}"));
            }
        }
        // The fixtures are a dev-dependency any crate may have.
        for dep in workspace_deps(&manifest, "dev-dependencies") {
            if !allowed.contains(&dep.as_str()) && dep != "tuimex-fixtures" {
                bad.push(format!("{name} -> {dep} (dev)"));
            }
        }
        for dep in workspace_deps(&manifest, "build-dependencies") {
            bad.push(format!("{name} -> {dep} (build)"));
        }
        assert!(manifest.get("target").is_none(), "{name} has target-specific dependencies this test does not read");
    }
    assert!(bad.is_empty(), "crate edges PLAN §2 does not allow:\n{}", bad.join("\n"));
}

/// A whole word of `text` starting at `i`, made of the characters `of` accepts.
fn word_at(text: &str, i: usize, of: impl Fn(char) -> bool) -> &str {
    let rest = &text[i..];
    &rest[..rest.find(|c: char| !of(c)).unwrap_or(rest.len())]
}

fn is_review_finding_id(word: &str) -> bool {
    let mut chars = word.chars();
    matches!(chars.next(), Some('R' | 'T'))
        && (1..=2).contains(&chars.as_str().len())
        && chars.all(|c| c.is_ascii_digit())
}

/// The owners of the repositories a document here may link to. A project
/// goes on this list only if the prior-art review did not read it.
const LINKED_OWNERS: [&str; 2] = ["lampseen", "rust-lang"];

/// ADR-052: the review of two other mail clients is published as classes
/// of flaw, with nothing that says where in whose code a weakness is. This
/// cannot hold their names without publishing them, so it looks for the
/// forms such a detail takes.
#[test]
fn nothing_published_points_at_a_weakness_in_another_project() {
    let all = files(|_| true);
    let rel = |p: &Path| p.strip_prefix(root()).unwrap().display().to_string();
    let ours: Vec<String> = all.iter().map(|p| rel(p)).collect();
    let path_char = |c: char| c.is_ascii_alphanumeric() || "_-./".contains(c);
    let mut bad = Vec::new();
    for f in all.iter().filter(|p| has_ext(p, &["md", "rs", "toml", "yml", "py", "sh", "mjs"])) {
        let (name, text) = (rel(f), fs::read_to_string(f).unwrap());
        for (n, line) in text.lines().enumerate() {
            let mut say = |what: String| bad.push(format!("{name}:{}: {what}", n + 1));

            // `<file>.rs:<line>` where the file is not one of this repository's.
            for (i, _) in line.match_indices(".rs:") {
                if !line[i + 4..].starts_with(|c: char| c.is_ascii_digit()) {
                    continue;
                }
                let start = line[..i].rfind(|c: char| !path_char(c)).map_or(0, |j| j + 1);
                let file = format!("{}.rs", &line[start..i]);
                if !ours.iter().any(|o| o == &file || o.ends_with(&format!("/{file}"))) {
                    say(format!("`{file}` and a line number, which is not a file here"));
                }
            }

            // A finding numbered as the review numbered them: in a table's
            // first cell, or after the word.
            let cell = line.strip_prefix("| ").and_then(|l| l.split(' ').next()).unwrap_or("");
            let after =
                line.match_indices("inding ").map(|(i, m)| word_at(line, i + m.len(), |c| c.is_ascii_alphanumeric()));
            for id in after.chain([cell]).filter(|w| is_review_finding_id(w)) {
                say(format!("finding `{id}`, the review's own numbering"));
            }

            for (i, m) in line.match_indices("github.com/") {
                let owner = word_at(line, i + m.len(), |c| c.is_ascii_alphanumeric() || c == '-');
                if !owner.is_empty() && !LINKED_OWNERS.contains(&owner) {
                    say(format!("a link to a repository of `{owner}`, who is not in LINKED_OWNERS"));
                }
            }

            // The review named the commits it read. Where the review is
            // told, nothing that reads as a commit hash is written.
            if name.starts_with("docs/history/") || name == "docs/decisions.md" {
                for w in line.split(|c: char| !c.is_ascii_alphanumeric()) {
                    let hex = w.chars().all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c));
                    if hex
                        && (7..=40).contains(&w.len())
                        && w.contains(|c: char| c.is_ascii_digit())
                        && w.contains(|c: char| c.is_ascii_lowercase())
                    {
                        say(format!("`{w}`, which reads as a commit hash"));
                    }
                }
            }
        }
    }
    assert!(bad.is_empty(), "details ADR-052 keeps out of what is published:\n{}", bad.join("\n"));
}

/// ADR-053: a harness that starts the binary gives it an environment of
/// its own, and in that environment the keyring is off. One that cleared
/// the environment and did not say so would leave the binary free to ask
/// the keyring of whoever runs the tests.
#[test]
fn every_harness_that_starts_the_binary_turns_the_keyring_off() {
    let mut harnesses = 0;
    for path in files(|p| has_ext(p, &["rs"]) && p.components().any(|c| c.as_os_str() == "tests")) {
        let text = fs::read_to_string(&path).unwrap();
        if !text.contains("CARGO_BIN_EXE_tuimex") || path.ends_with("docs.rs") {
            continue;
        }
        harnesses += 1;
        let (cleared, off) =
            (text.matches(".env_clear()").count(), text.matches(".env(\"TUIMEX_KEYRING\", \"none\")").count());
        assert!(cleared > 0, "{} starts the binary with the caller's environment", path.display());
        assert_eq!(cleared, off, "{}: each cleared environment turns the keyring off", path.display());
    }
    assert!(harnesses >= 4, "found only {harnesses} files that start the binary");
    assert!(read("Makefile").contains("\tTUIMEX_KEYRING=none cargo test --workspace --locked\n"));
}

/// Each job of a workflow with its body: a job is a key two spaces in,
/// under `jobs:`, and its body is every line up to the next one.
fn jobs(workflow: &str) -> Vec<(String, String)> {
    let text = read(&format!(".github/workflows/{workflow}"));
    let (_, under) = text.split_once("\njobs:\n").unwrap_or_else(|| panic!("{workflow} has no jobs"));
    let mut found: Vec<(String, String)> = Vec::new();
    for line in under.lines() {
        let name = line.strip_prefix("  ").and_then(|l| l.strip_suffix(':'));
        match name.filter(|n| n.chars().all(|c| c.is_ascii_lowercase() || c == '-') && !n.is_empty()) {
            Some(name) => found.push((name.to_string(), String::new())),
            None => {
                if let Some((_, body)) = found.last_mut() {
                    body.push_str(line);
                    body.push('\n');
                }
            }
        }
    }
    assert!(!found.is_empty(), "parsed no jobs out of {workflow}; a test of them asserts nothing");
    found
}

/// A required check no job reports leaves every pull request waiting for
/// good, and a job the ruleset does not name can fail without stopping a
/// merge. The file is the ruleset as it is to be applied.
#[test]
fn the_ruleset_requires_exactly_the_jobs_ci_has() {
    let ruleset: serde_json::Value = serde_json::from_str(&read(".github/rulesets/main.json")).unwrap();
    let rule = |kind: &str| {
        let rules = ruleset["rules"].as_array().unwrap();
        rules.iter().find(|r| r["type"] == kind).unwrap_or_else(|| panic!("the ruleset has no {kind} rule")).clone()
    };
    let checks = rule("required_status_checks")["parameters"]["required_status_checks"].as_array().unwrap().clone();
    let required: BTreeSet<String> = checks.iter().map(|c| c["context"].as_str().unwrap().to_string()).collect();
    let ci: BTreeSet<String> = jobs("ci.yml").into_iter().map(|(name, _)| name).collect();
    assert_eq!(required, ci, "the checks main requires, and the jobs ci.yml has");
    assert_eq!(required.len(), checks.len(), "a check is named twice");
    // GitHub Actions itself: a status of the same name from anything else
    // does not count.
    assert!(checks.iter().all(|c| c["integration_id"] == 15368), "{checks:?}");
    for kind in ["deletion", "non_fast_forward", "required_linear_history", "pull_request", "code_scanning"] {
        rule(kind);
    }
    assert_eq!(rule("pull_request")["parameters"]["allowed_merge_methods"], serde_json::json!(["squash", "rebase"]));
}

/// ci.yml lets a change that is only markdown skip the jobs that build.
/// Three must never be in that set: `scope` decides it, `check` runs the
/// tests in this file, which read the markdown, and `one-palette` scans it
/// for colour values. Every job has a time limit: one that hangs would
/// otherwise hold a runner for six hours.
#[test]
fn the_jobs_that_read_prose_are_not_skipped_for_prose_and_every_job_has_a_time_limit() {
    let always = ["scope", "check", "one-palette"];
    for (name, body) in jobs("ci.yml") {
        let gated =
            body.contains("    needs: scope\n") && body.contains("    if: needs.scope.outputs.code == 'true'\n");
        let half = body.contains("needs:") || body.contains("    if:");
        if always.contains(&name.as_str()) {
            assert!(!half, "the {name} job is gated, so a change that is only prose would not run it");
        } else {
            assert!(gated, "the {name} job is not gated on scope, so prose pays for it");
        }
    }
    let ci: Vec<String> = jobs("ci.yml").into_iter().map(|(name, _)| name).collect();
    assert!(always.iter().all(|a| ci.iter().any(|j| j == a)), "{ci:?}");
    for workflow in ["ci.yml", "release.yml", "outdated.yml"] {
        for (name, body) in jobs(workflow) {
            assert!(body.contains("\n    timeout-minutes: "), "{workflow}: the {name} job has no time limit");
        }
    }
}

/// The value of `key` on each line of the release workflow's build matrix.
fn matrix(key: &str) -> Vec<String> {
    let release = read(".github/workflows/release.yml");
    let rows: Vec<&str> = release.lines().filter(|l| l.trim_start().starts_with("- { target: ")).collect();
    let value = |row: &str| {
        let (_, rest) = row.split_once(&format!("{key}: ")).unwrap_or_else(|| panic!("no {key} in {row}"));
        rest.split([',', ' ']).next().unwrap().to_string()
    };
    rows.into_iter().map(value).collect()
}

/// ADR-051 and ADR-022: four targets, each on the runner the record gives
/// it, and nothing for Windows.
#[test]
fn the_release_builds_exactly_the_four_targets_and_none_for_windows() {
    let built: Vec<(String, String)> = matrix("target").into_iter().zip(matrix("os")).collect();
    let pair = |target: &str, os: &str| (target.to_string(), os.to_string());
    let wanted = [
        pair("x86_64-unknown-linux-gnu", "ubuntu-22.04"),
        pair("aarch64-unknown-linux-gnu", "ubuntu-22.04-arm"),
        pair("x86_64-apple-darwin", "macos-latest"),
        pair("aarch64-apple-darwin", "macos-latest"),
    ];
    assert_eq!(built, wanted);
    let release = read(".github/workflows/release.yml").to_lowercase();
    assert!(!release.contains("windows") && !release.contains(".exe") && !release.contains(".zip"));
    // A record's lines are wrapped where they fall.
    let records = read("docs/decisions.md").replace('\n', " ");
    for (target, os) in wanted {
        assert!(records.contains(&format!("`{target}` on `{os}`")), "no record says {target} is built on {os}");
    }
}

/// The formula is filled in from a release's checksums, so an archive it
/// names that no release holds is a formula that cannot be installed.
#[test]
fn every_archive_the_formula_names_is_one_the_release_makes() {
    let made: BTreeSet<String> = matrix("name").into_iter().map(|name| format!("tuimex_{name}.tar.gz")).collect();
    assert_eq!(made.len(), 4, "{made:?}");
    let release = read(".github/workflows/release.yml");
    assert!(release.contains("tar -czf dist/tuimex_${{ matrix.name }}.tar.gz "), "the archive is named by the matrix");
    let formula = read("packaging/homebrew/tuimex.rb");
    let prefix = "https://github.com/lampseen/tuimex/releases/download/vVERSION/";
    let named: BTreeSet<String> = formula
        .lines()
        .filter_map(|l| l.trim().strip_prefix("url \""))
        .map(|url| url.trim_end_matches('"').strip_prefix(prefix).unwrap_or_else(|| panic!("{url}")).to_string())
        .collect();
    assert_eq!(named, made, "what the formula downloads, and what a release holds");
    assert_eq!(formula.matches("sha256 \"SHA256_").count(), 4);
}

/// The install pages tell a reader which file to download. A name there
/// that no release holds is an instruction that ends in a 404.
#[test]
fn every_archive_the_readme_and_the_wiki_name_is_one_the_release_makes() {
    let made: BTreeSet<String> = matrix("name").into_iter().map(|name| format!("tuimex_{name}.tar.gz")).collect();
    let mut named = BTreeSet::new();
    for page in ["README.md", "wiki/Installing.md"] {
        let text = read(page);
        for (i, _) in text.match_indices("tuimex_") {
            let name = word_at(&text, i, |c| c.is_ascii_alphanumeric() || c == '_' || c == '.');
            if name.ends_with(".tar.gz") {
                assert!(made.contains(name), "{page} names {name}, which no release holds");
                named.insert(name.to_string());
            }
        }
    }
    assert_eq!(named, made, "every archive a release holds is offered to the reader");
}

/// PLAN §9 says when the weekly check runs; the workflow is what runs it.
#[test]
fn the_weekly_check_runs_when_the_plan_says() {
    let outdated = read(".github/workflows/outdated.yml");
    let crons: Vec<&str> = outdated.lines().filter_map(|l| l.trim().strip_prefix("- cron: ")).collect();
    assert_eq!(crons, ["'31 6 * * 1'"], "one schedule: minute 31, hour 6, any day of any month, Monday");
    assert!(read("docs/PLAN.md").contains("Mondays at 06:31 UTC"), "PLAN §9 names another time");
    assert!(read("docs/decisions.md").contains("Mondays at 06:31 UTC"), "no record names that time");
}

const REPO: &str = "https://github.com/lampseen/tuimex/";

fn wiki_pages() -> Vec<String> {
    let names = fs::read_dir(root().join("wiki")).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap());
    let mut pages: Vec<String> = names.filter_map(|n| n.strip_suffix(".md").map(String::from)).collect();
    pages.sort();
    assert!(pages.len() > 10, "found only {} wiki pages", pages.len());
    pages
}

/// The target of every markdown link in `text`.
fn link_targets(text: &str) -> Vec<&str> {
    let mut found = Vec::new();
    let mut rest = text;
    while let Some(i) = rest.find("](") {
        rest = &rest[i + 2..];
        let Some(end) = rest.find(')') else { break };
        found.push(&rest[..end]);
    }
    found
}

/// The anchor GitHub gives a heading: lower case, spaces as hyphens, and
/// nothing else but letters, digits, hyphens and underscores.
fn anchors(markdown: &str) -> BTreeSet<String> {
    let headings = markdown.lines().filter(|l| l.starts_with('#')).map(|l| l.trim_start_matches('#').trim());
    let slug = |heading: &str| {
        let kept = heading.to_lowercase().replace(' ', "-");
        kept.chars().filter(|c| c.is_alphanumeric() || matches!(c, '-' | '_')).collect()
    };
    headings.map(slug).collect()
}

/// Whatever follows each `opening` in `text`, as far as `more` allows.
fn following<'a>(text: &'a str, opening: &str, more: impl Fn(char) -> bool + Copy) -> BTreeSet<&'a str> {
    let after = text.match_indices(opening).map(|(i, _)| &text[i + opening.len()..]);
    after.map(|rest| &rest[..rest.find(|c| !more(c)).unwrap_or(rest.len())]).filter(|word| !word.is_empty()).collect()
}

/// The wiki's Home is its index: a page it does not list is a page nobody
/// finds, and a link to a page that is not there is a dead end.
#[test]
fn home_indexes_every_wiki_page() {
    let (home, pages) = (read("wiki/Home.md"), wiki_pages());
    let linked: BTreeSet<&str> = link_targets(&home).into_iter().filter(|t| !t.contains(['/', '#', '.'])).collect();
    for page in pages.iter().filter(|p| *p != "Home") {
        assert!(linked.contains(page.as_str()), "wiki/Home.md does not link to {page}");
    }
    for target in linked {
        assert!(pages.iter().any(|p| p == target), "wiki/Home.md links to {target}, which is not a page");
    }
}

/// A wiki page names another as `Page-name`, and a section by the anchor
/// its heading is given. The published wiki has no redirects, so a
/// renamed page or heading is a 404 nothing else would notice.
#[test]
fn every_wiki_link_resolves() {
    let pages = wiki_pages();
    let (mut broken, mut seen) = (Vec::new(), 0);
    for page in &pages {
        let text = read(&format!("wiki/{page}.md"));
        for target in link_targets(&text).into_iter().filter(|t| !t.contains("://") && !t.starts_with("mailto:")) {
            seen += 1;
            let (name, anchor) = target.split_once('#').map_or((target, None), |(n, a)| (n, Some(a)));
            let name = if name.is_empty() { page.as_str() } else { name };
            if !pages.iter().any(|p| p == name) {
                broken.push(format!("{page}: {target} is not a page"));
            } else if anchor.is_some_and(|a| !anchors(&read(&format!("wiki/{name}.md"))).contains(a)) {
                broken.push(format!("{page}: {target} names a heading {name} does not have"));
            }
        }
    }
    assert!(seen >= 40, "found only {seen} links between wiki pages");
    assert!(broken.is_empty(), "wiki links that lead nowhere:\n{}", broken.join("\n"));
}

/// The wiki is another repository, so its links into this one are whole
/// addresses, which the relative-link test passes over. The README's are
/// checked alike.
#[test]
fn wiki_links_into_this_repository_resolve() {
    let mut texts: Vec<(String, String)> =
        wiki_pages().into_iter().map(|p| (format!("wiki/{p}.md"), read(&format!("wiki/{p}.md")))).collect();
    texts.push(("README.md".into(), read("README.md")));
    let (mut broken, mut seen) = (Vec::new(), 0);
    for (file, text) in &texts {
        for kind in ["blob/main/", "tree/main/"] {
            for target in following(text, &format!("{REPO}{kind}"), |c| !c.is_whitespace() && c != ')') {
                seen += 1;
                let (path, anchor) = target.split_once('#').map_or((target, None), |(p, a)| (p, Some(a)));
                if !root().join(path).exists() {
                    broken.push(format!("{file}: {path} is not in this repository"));
                } else if anchor.is_some_and(|a| !anchors(&read(path)).contains(a)) {
                    broken.push(format!("{file}: {target} names a heading {path} does not have"));
                }
            }
        }
    }
    assert!(seen >= 5, "found only {seen} links from the wiki into this repository");
    assert!(broken.is_empty(), "links into this repository that lead nowhere:\n{}", broken.join("\n"));
}

/// The README is a front door into the wiki: each of its sections points
/// at a page, by an address the relative-link test cannot follow.
#[test]
fn every_wiki_url_in_the_readme_names_a_page() {
    let (readme, pages) = (read("README.md"), wiki_pages());
    let targets = following(&readme, &format!("{REPO}wiki/"), |c| c.is_alphanumeric() || matches!(c, '-' | '#'));
    for target in &targets {
        let (page, anchor) = target.split_once('#').map_or((*target, None), |(p, a)| (p, Some(a)));
        assert!(pages.iter().any(|p| p == page), "the README links to wiki page {page}, which wiki/ does not have");
        if let Some(anchor) = anchor {
            let has = anchors(&read(&format!("wiki/{page}.md"))).contains(anchor);
            assert!(has, "the README links to {target}; that page has no such heading");
        }
    }
    let sections = readme.lines().filter(|l| l.starts_with("## ")).count();
    assert!(targets.len() >= sections, "the README has {sections} sections and links to {} wiki pages", targets.len());
}

/// The README's screen is a golden file, which the renderer is compared
/// with cell for cell (ADR-002): so the screen a visitor sees first is one
/// tuimex draws. Trailing spaces are the golden's padding to its width.
#[test]
fn the_readme_s_screen_is_the_list_golden() {
    let trimmed =
        |text: &str| text.lines().map(str::trim_end).collect::<Vec<_>>().join("\n").trim_matches('\n').to_string();
    let readme = read("README.md");
    // The fence that closes the command's block, the one that opens the
    // screen's, then the screen.
    let block = readme.split("```\n").nth(2).expect("a screen in the README, after the command");
    assert_eq!(trimmed(block), trimmed(&read("tests/golden/list-80-ascii.txt")));
}

/// The wiki's Commands page is the reference for both command lines. A
/// subcommand is whatever `tuimex --help` lists, and a `:` command is a
/// row of the table completion is drawn from; each must be on the page,
/// and the page may name no other.
#[test]
fn the_wiki_s_commands_are_the_commands_there_are() {
    let page = read("wiki/Commands.md");
    let word = |c: char| c.is_ascii_lowercase() || c == '-';

    let help = Command::new(env!("CARGO_BIN_EXE_tuimex")).arg("--help").output().unwrap();
    let help = String::from_utf8(help.stdout).unwrap();
    let listed = help.split("Commands:\n").nth(1).and_then(|rest| rest.split("\n\n").next()).expect("a command list");
    let real: BTreeSet<&str> =
        listed.lines().filter_map(|l| l.split_whitespace().next()).filter(|c| *c != "help").collect();
    assert!(real.len() >= 10, "`tuimex --help` lists only {real:?}");
    let named: BTreeSet<&str> =
        following(&page, "`tuimex ", word).into_iter().filter(|w| !w.starts_with('-')).collect();
    assert_eq!(named, real, "the subcommands wiki/Commands.md names, and those `tuimex --help` lists");

    let real: BTreeSet<&str> = tuimex_ui::command::COMMANDS.iter().map(|c| c.name).collect();
    assert_eq!(following(&page, "`:", word), real, "the `:` commands wiki/Commands.md names, and those there are");
    let choices =
        tuimex_ui::command::COMMANDS.iter().flat_map(|c| c.choices.iter().map(move |choice| (c.name, choice.name)));
    for (command, choice) in choices {
        assert!(page.contains(&format!("`:{command} {choice}")), "wiki/Commands.md does not name :{command} {choice}");
    }
}

/// `docs/keymap.md` is written from the table the keys are read from; the
/// wiki's Keys page is the short form of it, and may name no key that
/// table does not have.
#[test]
fn every_key_on_the_wiki_s_keys_page_is_in_the_keymap() {
    let cell = |line: &str| line.strip_prefix("| `").and_then(|rest| rest.split(" | ").next()).map(str::to_string);
    let keys = |text: &str| -> BTreeSet<String> {
        let cells = text.lines().filter_map(cell);
        cells.flat_map(|c| c.replace('`', " ").split_whitespace().map(String::from).collect::<Vec<_>>()).collect()
    };
    let (known, named) = (keys(&read("docs/keymap.md")), keys(&read("wiki/Keys.md")));
    assert!(named.len() >= 40, "found only {} keys on wiki/Keys.md", named.len());
    let unknown: Vec<&String> = named.iter().filter(|k| *k != "then" && !known.contains(*k)).collect();
    assert!(unknown.is_empty(), "wiki/Keys.md names keys docs/keymap.md does not have: {unknown:?}");
}

/// A claim on the wiki's What-is-proven page is only as good as the test
/// beside it, and a test that was renamed or removed proves nothing. A
/// name there is a test function somewhere under `crates/`, and a corpus
/// file is one under `tests/corpus/`.
#[test]
fn every_test_the_wiki_says_proves_something_exists() {
    let page = read("wiki/What-is-proven.md");
    let quoted: BTreeSet<&str> = page.split('`').skip(1).step_by(2).collect();
    let is_test =
        |q: &&str| q.contains('_') && q.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    let tests: Vec<&str> = quoted.iter().copied().filter(is_test).collect();
    assert!(tests.len() >= 150, "found only {} test names on wiki/What-is-proven.md", tests.len());
    let code: Vec<String> = files(|p| has_ext(p, &["rs"])).iter().map(|p| fs::read_to_string(p).unwrap()).collect();
    let missing: Vec<&str> =
        tests.into_iter().filter(|t| !code.iter().any(|c| c.contains(&format!("fn {t}(")))).collect();
    assert!(missing.is_empty(), "wiki/What-is-proven.md names tests that do not exist:\n{}", missing.join("\n"));
    for corpus in quoted.iter().filter(|q| q.ends_with(".eml")) {
        assert!(root().join("tests/corpus").join(corpus).exists(), "there is no tests/corpus/{corpus}");
    }
}
