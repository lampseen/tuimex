# Publishing on GitHub

How tuimex goes from a local repository to the public
`github.com/lampseen/tuimex`, with the process around it that
[bothy](https://github.com/lampseen/bothy) and
[pullpen](https://github.com/lampseen/pullpen) have. It is a checklist:
each step says what to run and how to tell it worked, and carries its
state.

**Where it stands on 2026-11-24: sections 2 to 8 and 10 are done but for
the steps marked otherwise; 9 is not.** The repository is public, every
job of `ci.yml` has passed on `main`, the ruleset is on, and `v0.1.0` is
tagged and published: the release workflow built, attested and published
all four archives at its first attempt (run 45454590282). Each step
carries the run, pull request, issue or ruleset that shows it; a step with
none says what is unverified.

[What the launch taught](#what-the-launch-taught), at the end, is for the
next project that follows this list.

Decided already: the repository is **public**, like its siblings. On a
free plan the wiki, branch rulesets, CodeQL and artifact attestations work
only on public repositories.

## 1. What must be true locally first

- [x] **`make check` passes on `main`**, with the documentation, the
  workflows and the ruleset merged into it. *Done on a runner 2026-11-24:
  the job `check`, which runs it, passed on `main` at 9f9b675 (run
  45436881760). At the commit first pushed it failed there, see section 6.
  The local run before the push is not recorded here.*
- [x] **`make real-server` passes**, since CI will run it and it is the
  one gate `make check` does not include. It needs a container runtime.
  *Done on a runner 2026-11-24, in the same run, under Docker. The local
  run before the push is not recorded here.*
- [ ] **Nothing private is in the tree or its history.** No real address,
  no account name of the owner's, no message. The fixtures test scans
  `tests/` and the fixture crate; read `git log -p -- docs/` once by eye
  for the rest, because the first push publishes every commit. *`main` was
  pushed as one commit, 8ec6555, so no earlier history was published, and
  the fixtures test passes in `check`. The reading by eye is not recorded:
  unverified.*
- [x] **No third-party project is blamed by name** in `docs/`. *Done as
  far as a test holds it:
  `nothing_published_points_at_a_weakness_in_another_project`, in `check`
  (ADR-052).*
- [ ] **The README, the wiki and `What-is-proven` describe this commit.**
  `tests/docs.rs` holds the links, the commands, the keys and the test
  names; the prose is read once against the code. *The tests pass in
  `check`. A reading of the prose against the published commit is not
  recorded: unverified.*

## 2. Create the repository

- [x] **Create it public, with the wiki on.** *Done 2026-11-24:
  `gh api repos/lampseen/tuimex --jq '{visibility,has_wiki}'` answers
  `public` and `true`.*

  ```sh
  gh repo create lampseen/tuimex --public \
    --description "A terminal mail client: clear an inbox from the keyboard, across accounts, where nothing a message contains can act for you or report on you."
  ```

  The wiki is on by default; `gh repo view lampseen/tuimex --json hasWikiEnabled`
  says so.

- [ ] **Set the merge settings as the siblings have them**: auto-merge
  allowed, branches deleted on merge, squash and rebase only, squash
  commits titled from the pull request. *Done 2026-11-24: the repository
  answers with each value the command sets.*

  ```sh
  gh api -X PATCH repos/lampseen/tuimex \
    -F allow_auto_merge=true -F delete_branch_on_merge=true \
    -F allow_merge_commit=false -F allow_squash_merge=true -F allow_rebase_merge=true \
    -f squash_merge_commit_title=PR_TITLE -f squash_merge_commit_message=PR_BODY
  ```

  Compare with a sibling:
  `gh api repos/lampseen/pullpen --jq '{allow_auto_merge,delete_branch_on_merge,allow_merge_commit,squash_merge_commit_title}'`.

## 3. The first push

- [ ] **Push `main`.** It is the one push that lands without a pull
  request, because there is no base for one yet. Everything after it goes
  through a pull request. *Done 2026-11-24: one commit, 8ec6555.*

  ```sh
  git remote add origin https://github.com/lampseen/tuimex.git
  git push -u origin main
  ```

  The remote was added over https. An SSH address does as well where a
  key is registered.

  Push no other local branch and no worktree's branch: they are working
  history, and `main` already holds what they became.

## 4. Reporting and scanning

- [ ] **Turn on private vulnerability reporting**, which is what the link
  in `SECURITY.md` and in the issue chooser opens. *Done 2026-11-24:
  `gh api repos/lampseen/tuimex/private-vulnerability-reporting` answers
  `{"enabled":true}`. The form seen signed out, below: not recorded.*

  ```sh
  gh api -X PUT repos/lampseen/tuimex/private-vulnerability-reporting
  ```

  Then open `https://github.com/lampseen/tuimex/security/advisories/new`
  signed in as someone else, or signed out, and see a form and not a 404.

- [x] **Turn on CodeQL default setup.** *Done 2026-11-24, without Rust:
  `configured`, weekly, for `actions`, `javascript-typescript` and
  `python`.*

  ```sh
  gh api -X PATCH repos/lampseen/tuimex/code-scanning/default-setup -f state=configured
  gh api repos/lampseen/tuimex/code-scanning/default-setup --jq '{state,languages,schedule}'
  ```

  **See** `configured` and the languages GitHub chose. `rust` is not among
  them, and the API did not take it as a value when it was asked for, so
  **CodeQL does not scan the Rust code**: what it reads here is the
  workflows, `design/tui.js` and the Python helpers of the tests. Rust
  would be added under Settings → Code security, in the browser. *Not
  done.* No document may say the Rust is scanned until that answer names
  `rust`.

## 5. Seed the wiki

GitHub makes a wiki's repository when its first page is saved in the
browser, and in no other way.

- [ ] **Save one placeholder page by hand**: the repository's Wiki tab →
  *Create the first page* → *Save*. *Done 2026-11-24. Before it, the run
  of `wiki.yml` that the first push started failed with `Repository not
  found` for the wiki (run 45431025966), which is that rule seen.*
- [x] **Run the copy**: `gh workflow run wiki.yml`, then
  `gh run watch`. **See** `pushed N pages`, one for each file in `wiki/`,
  and the wiki's Home with the words table. *Done 2026-11-24: `pushed 17
  pages` for the 17 files (run 45431618841), and Home has the table.*
- [x] If the push is refused with a 403, the workflow's own token does not
  reach the wiki: make a fine-grained token with Contents: write on this
  repository, save it as the secret `WIKI_TOKEN`, and run it again. *Not
  needed: the workflow's own token pushed.*
- [ ] **Follow three links** on the published wiki by hand: one between
  pages, one with an anchor, one into `docs/`. The tests resolve them
  against files; only the browser shows what GitHub made of them. *Not
  done.*

## 6. The first CI runs

The push in step 3 starts `ci.yml`, and a runner is not this machine.

- [x] **Read every job's result**: `gh run list --workflow ci.yml`, then
  `gh run view <id> --log-failed` for each that failed. *Done 2026-11-24:
  the first run (45431025925) failed in `check`, `macos` and `real-server`
  and passed in the other five. Each failure was a test's assumption about
  the machine and none a fault in tuimex; pull request #1 fixed all three.*
- [x] **Expect the `macos` job to find something.** Fix what it finds in
  pull requests, with a test where a test can hold it; do not skip a test
  on macOS without a record saying why. *Done 2026-11-24: it found what
  `check` found, three assertions that expected a saved file's path whole
  where the checkout is under the home directory. With #1 the whole suite
  passes on `macos-latest`, run with `--no-fail-fast`, and no test is
  skipped there.*
- [x] **Expect `real-server` to differ from the local run**: the runner
  uses `docker` where this machine uses `podman`, and pulls its two images
  fresh. *Done 2026-11-24: Docker made no difference; the runner's Python
  did, see the end of this page. It passes on a runner with #1.*
- [x] **Every job green on `main`, once, before any check is required.** A
  ruleset that requires a job that has never passed locks `main`. *Done
  2026-11-24: run 45436881760 at 9f9b675, at its second attempt. In the
  first GitHub cancelled `macos` after fifteen minutes with no runner to
  give it, which is its capacity and not a failure.*
- [x] **Then correct the pages that speak of CI in the future**:
  `wiki/Where-it-runs.md` and `wiki/What-is-proven.md` say what the
  `macos` job has shown, with the date. *Done 2026-11-24.*

## 7. The ruleset

- [x] **Apply it from the file.** *Done 2026-11-24, after the green run
  above: ruleset 28003326.*

  ```sh
  gh api -X POST repos/lampseen/tuimex/rulesets --input .github/rulesets/main.json
  ```

- [ ] **Compare what is live with the file**, because the API accepts a
  ruleset and may drop what it does not understand. *Done 2026-11-24:
  every field the file has (`name`, `target`, `enforcement`, `conditions`,
  `rules`, `bypass_actors`) is equal in what the API returns for
  28003326.*

  ```sh
  id=$(gh api repos/lampseen/tuimex/rulesets --jq '.[] | select(.name=="main") | .id')
  gh api repos/lampseen/tuimex/rulesets/$id --jq '{enforcement, rules: [.rules[].type], checks: [.rules[] | select(.type=="required_status_checks") | .parameters.required_status_checks[].context]}'
  ```

  **See** `enforcement: active`, the rule types the file has, and exactly
  the job names the file requires.

- [ ] **Prove it holds**: `git push origin main` with a new commit is
  refused, and a pull request with a failing check cannot be merged. *Not
  recorded: unverified. Pull request #3 was merged under the ruleset with
  every check green, which shows what it lets through and not what it
  refuses.*
- [x] From here `CONTRIBUTING.md`'s "`main` requires a pull request" is
  true of the repository and not only of the file. *The live ruleset has
  the `pull_request` rule.*

## 8. The release dry run

A tag cannot be taken back, so the first release is not the first time the
release workflow runs.

- [ ] **Start it by hand**, which builds and packages every target and
  publishes nothing. *Done 2026-11-24, twice: run 45440706183 on `main`
  took three attempts, each of the first two losing one of its two Mac
  jobs to the runner queue; pull request #3 made one Mac job build both
  Mac targets, and run 45448989344, from its branch, passed at the first
  attempt. In both the attest and publish steps were skipped, as a dry run
  skips them; they first ran on the tag, in section 10.*

  ```sh
  gh workflow run release.yml
  gh run watch
  ```

- [x] **See four archives** in the run, named as `wiki/Installing.md`
  names them: `tuimex_linux_amd64.tar.gz`, `tuimex_linux_arm64.tar.gz`,
  `tuimex_darwin_amd64.tar.gz`, `tuimex_darwin_arm64.tar.gz`. *Done
  2026-11-24: run 45448989344 uploads them as one artifact for each of its
  three runners, and its `release` job counts four lines in
  `checksums.txt`.*
- [ ] **Download the one for a machine you have**, unpack it, and run
  `tuimex --version` and `tuimex --fixtures`. On a Mac, note here what
  Gatekeeper said and whether `wiki/Installing.md` describes it. *Not
  done: no archive of a dry run has been unpacked and started by hand. The
  published Linux x86-64 archive was unpacked and answered `--version` on
  2026-11-24, in section 10; `--fixtures` from it is not recorded, and no
  archive has been unpacked or started on a Mac.*

## 9. The by-hand script

- [ ] **Run `docs/testing.md`** against accounts of your own, with the
  release build, and add its rows to the record at its end. It is the only
  proof there will be for Gmail as Google runs it, Proton Bridge and the
  system keyring. *Not done: its one row is "set up and synced" on
  2026-11-23.*
- [ ] **Bring `wiki/Where-it-runs.md` and `wiki/What-is-proven.md` level
  with that record**, in a pull request. *Not done.*

## 10. The release

`Cargo.toml` already says 0.1.0, so the first release needs no bump:
`make release VERSION=x.y.z` is for the ones after it, and has opened no
bump pull request yet: unverified against GitHub.

- [x] **Tag it.** *Done 2026-11-24: `make release-tag` pushed `v0.1.0` at
  c8d9559, the head of `main`, where `ci.yml` had passed (run
  45453808396).*

  ```sh
  git switch main && git pull
  make release-tag
  ```

- [x] **Watch the workflow**, then check what it published, from a clean
  directory. *Done 2026-11-24: run 45454590282 passed at its first
  attempt, in `check`, the three build jobs and `release`, whose attest and
  publish steps ran. The release "tuimex 0.1.0" is published, neither a
  draft nor a pre-release, with six assets: the four archives,
  `checksums.txt` and `attestation.jsonl`. On a Linux x86-64 machine the
  download below succeeded, and `checksums.txt` beside it;
  `sha256sum -c --ignore-missing checksums.txt` said
  `tuimex_linux_amd64.tar.gz: OK`; `gh attestation verify` exited 0 for the
  archive and for `checksums.txt`, and its JSON names the predicate
  `https://slsa.dev/provenance/v1`, five subjects (the four archives and
  `checksums.txt`) and `.github/workflows/release.yml` at
  `refs/tags/v0.1.0` in this repository; a copy of the archive with one
  byte appended failed it with exit 1; the archive holds `tuimex`,
  `LICENSE` and `README.md`, and the binary printed `tuimex 0.1.0`. The
  other three archives' links answered 200 and no more was asked of them:
  neither Mac archive has been unpacked or run by anyone, and the Linux
  arm64 one has not been run on an arm64 machine.*

  ```sh
  curl -LO https://github.com/lampseen/tuimex/releases/latest/download/tuimex_linux_amd64.tar.gz
  gh attestation verify tuimex_linux_amd64.tar.gz --repo lampseen/tuimex
  ```

  **See** the download succeed under that fixed name, and the attestation
  name this repository's release workflow. On `v0.1.0` it was the first
  time the attest and publish steps ran at all.

- [ ] **Read the README's Install section and `wiki/Installing.md` once
  more against the release page.** They were written before a release
  existed and worded to be true on this day. *Read 2026-11-24 against the
  release's assets: the archive names, the `latest` links and the
  attestation command are what the release holds and what was run above on
  Linux x86-64, and the sentence in `wiki/Installing.md` that explained a
  404 by there being no release went. What both say of macOS has been
  tried by nobody: unverified.*
- [ ] **The Homebrew formula** in `packaging/homebrew/` is a template; a
  tap is a later step and the README says "to follow" until it exists.
  *Not done: its version and checksums are still placeholders, there is
  no tap, and nothing has installed from it.*

## The weekly check

- [x] **Start `outdated.yml` once by hand**, so the first Monday is not
  the first time it runs: `gh workflow run outdated.yml`. **See** an issue
  for what `cargo update` would change, if anything would, and one for
  advisories only if the audit found any. *Done 2026-11-24: run
  45439993307 opened issue #2, "Dependency updates available", and no
  advisory issue, the audit having found no vulnerability. The schedule
  itself has not fired yet.*

## What the launch taught

For the next project that follows this list. Each cost a failed run here.

- **A runner's checkout is under the home directory**, and a developer's
  often is not. A program that prints a path under home with a tilde
  passes its tests in one place and fails them in the other. Run the suite
  once with `HOME` set above the checkout before the first push.
- **The runner's Python is not this machine's.** `imaplib` quotes a
  mailbox name itself from 3.15; before it, a name with a space goes out
  as two words. A test helper quotes where the library does not, and the
  jobs print `python3 --version` so the log says which it was.
- **Mac runners are scarce.** GitHub cancels a job that waits fifteen
  minutes for one ("The job was not acquired by Runner of type hosted"),
  and that is a cancelled job, not a failed one: run it again. A workflow
  that needs two Mac jobs at once loses one of them twice as often, so one
  Mac job builds both Mac targets (ADR-051's note).
- **Give the one Mac job `--no-fail-fast`.** With no Mac at hand, every
  failing test is wanted from a single run.
- **The ruleset goes on last**, after every job it requires has passed on
  `main` once, and CodeQL is turned on before it, because its code
  scanning rule waits for a result.
- **The first push starts every workflow that listens for one**, `wiki.yml`
  among them, before the wiki exists. That run fails, and is expected to.
- **Check the action majors before the first push.** A major built for a
  Node that GitHub has deprecated still runs, with a warning; the first
  runs here had it for `actions/checkout@v4`, and #1 moved the actions to
  majors on a supported Node.
- **CodeQL's default setup chooses its own languages.** Read what it
  answers, and claim no more than that.
