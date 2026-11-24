# Publishing on GitHub

How tuimex goes from a local repository to the public
`github.com/lampseen/tuimex`, with the process around it that
[bothy](https://github.com/lampseen/bothy) and
[pullpen](https://github.com/lampseen/pullpen) have. It is a checklist:
each step says what to run and how to tell it worked, and carries its
state.

**Where it stands: nothing below section 1 has been done.** The repository
does not exist on GitHub, no workflow has run there, and no release has
been cut. The `gh` commands were written from GitHub's documented API and
none has been run for this repository; where one is refused, record what
GitHub said here and go by that.

Decided already: the repository is **public**, like its siblings. On a
free plan the wiki, branch rulesets, CodeQL and artifact attestations work
only on public repositories.

## 1. What must be true locally first

- [ ] **`make check` passes on `main`**, with the documentation, the
  workflows and the ruleset merged into it. *Not done: they are on
  branches.*
- [ ] **`make real-server` passes**, since CI will run it and it is the
  one gate `make check` does not include. It needs a container runtime.
  *Not done on the merged `main`.*
- [ ] **Nothing private is in the tree or its history.** No real address,
  no account name of the owner's, no message. The fixtures test scans
  `tests/` and the fixture crate; read `git log -p -- docs/` once by eye
  for the rest, because the first push publishes every commit. *Not done.*
- [ ] **No third-party project is blamed by name** in `docs/`. *Not done:
  the rewrite of the history documents is on a branch.*
- [ ] **The README, the wiki and `What-is-proven` describe this commit.**
  `tests/docs.rs` holds the links, the commands, the keys and the test
  names; the prose is read once against the code. *Done for the branch
  that wrote them; to be read again after the merge.*

## 2. Create the repository

- [ ] **Create it public, with the wiki on.** *Not done.*

  ```sh
  gh repo create lampseen/tuimex --public \
    --description "A terminal mail client for clearing an inbox across several accounts, where nothing a message contains can act for you or report on you."
  ```

  The wiki is on by default; `gh repo view lampseen/tuimex --json hasWikiEnabled`
  says so.

- [ ] **Set the merge settings as the siblings have them**: auto-merge
  allowed, branches deleted on merge, squash and rebase only, squash
  commits titled from the pull request. *Not done.*

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
  through a pull request. *Not done.*

  ```sh
  git remote add origin git@github.com:lampseen/tuimex.git
  git push -u origin main
  ```

  Push no other local branch and no worktree's branch: they are working
  history, and `main` already holds what they became.

## 4. Reporting and scanning

- [ ] **Turn on private vulnerability reporting**, which is what the link
  in `SECURITY.md` and in the issue chooser opens. *Not done.*

  ```sh
  gh api -X PUT repos/lampseen/tuimex/private-vulnerability-reporting
  ```

  Then open `https://github.com/lampseen/tuimex/security/advisories/new`
  signed in as someone else, or signed out, and see a form and not a 404.

- [ ] **Turn on CodeQL default setup.** *Not done.*

  ```sh
  gh api -X PATCH repos/lampseen/tuimex/code-scanning/default-setup -f state=configured
  gh api repos/lampseen/tuimex/code-scanning/default-setup --jq '{state,languages}'
  ```

  See `rust` and `actions` among the languages. If the API refuses, say so
  here instead of assuming it is on.

## 5. Seed the wiki

GitHub makes a wiki's repository when its first page is saved in the
browser, and in no other way.

- [ ] **Save one placeholder page by hand**: the repository's Wiki tab →
  *Create the first page* → *Save*. *Not done.*
- [ ] **Run the copy**: `gh workflow run wiki.yml`, then
  `gh run watch`. **See** `pushed N pages`, one for each file in `wiki/`,
  and the wiki's Home with the words table. *Not done.*
- [ ] If the push is refused with a 403, the workflow's own token does not
  reach the wiki: make a fine-grained token with Contents: write on this
  repository, save it as the secret `WIKI_TOKEN`, and run it again. *Not
  known to be needed.*
- [ ] **Follow three links** on the published wiki by hand: one between
  pages, one with an anchor, one into `docs/`. The tests resolve them
  against files; only the browser shows what GitHub made of them. *Not
  done.*

## 6. The first CI runs

The push in step 3 starts `ci.yml`. It has never run on a runner, and a
runner is not this machine.

- [ ] **Read every job's result**: `gh run list --workflow ci.yml`, then
  `gh run view <id> --log-failed` for each that failed. *Not done.*
- [ ] **Expect the `macos` job to find something.** It is the first time
  the suite has run on a Mac. Fix what it finds in pull requests, with a
  test where a test can hold it; do not skip a test on macOS without a
  record saying why. *Not done.*
- [ ] **Expect `real-server` to differ from the local run**: the runner
  uses `docker` where this machine uses `podman`, and pulls its two images
  fresh. *Not done.*
- [ ] **Every job green on `main`, once, before any check is required.** A
  ruleset that requires a job that has never passed locks `main`. *Not
  done.*
- [ ] **Then correct the pages that speak of CI in the future**:
  `wiki/Where-it-runs.md` and `wiki/What-is-proven.md` say macOS rests on
  the `macos` job; once it has passed, they may say that it has, with the
  date. *Not done.*

## 7. The ruleset

- [ ] **Apply it from the file.** *Not done.*

  ```sh
  gh api -X POST repos/lampseen/tuimex/rulesets --input .github/rulesets/main.json
  ```

- [ ] **Compare what is live with the file**, because the API accepts a
  ruleset and may drop what it does not understand. *Not done.*

  ```sh
  id=$(gh api repos/lampseen/tuimex/rulesets --jq '.[] | select(.name=="main") | .id')
  gh api repos/lampseen/tuimex/rulesets/$id --jq '{enforcement, rules: [.rules[].type], checks: [.rules[] | select(.type=="required_status_checks") | .parameters.required_status_checks[].context]}'
  ```

  **See** `enforcement: active`, the rule types the file has, and exactly
  the job names the file requires.

- [ ] **Prove it holds**: `git push origin main` with a new commit is
  refused, and a pull request with a failing check cannot be merged. *Not
  done.*
- [ ] From here `CONTRIBUTING.md`'s "`main` requires a pull request" is
  true of the repository and not only of the file.

## 8. The release dry run

A tag cannot be taken back, so the first release is not the first time the
release workflow runs.

- [ ] **Start it by hand**, which builds and packages every target and
  publishes nothing. *Not done.*

  ```sh
  gh workflow run release.yml
  gh run watch
  ```

- [ ] **See four archives** among the run's artifacts, named as
  `wiki/Installing.md` names them: `tuimex_linux_amd64.tar.gz`,
  `tuimex_linux_arm64.tar.gz`, `tuimex_darwin_amd64.tar.gz`,
  `tuimex_darwin_arm64.tar.gz`. *Not done.*
- [ ] **Download the one for a machine you have**, unpack it, and run
  `tuimex --version` and `tuimex --fixtures`. On a Mac, note here what
  Gatekeeper said and whether `wiki/Installing.md` describes it. *Not
  done.*

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
`make release VERSION=x.y.z` is for the ones after it.

- [ ] **Tag it.** *Not done.*

  ```sh
  git switch main && git pull
  make release-tag
  ```

- [ ] **Watch the workflow**, then check what it published, from a clean
  directory. *Not done.*

  ```sh
  curl -LO https://github.com/lampseen/tuimex/releases/latest/download/tuimex_linux_amd64.tar.gz
  gh attestation verify tuimex_linux_amd64.tar.gz --repo lampseen/tuimex
  ```

  **See** the download succeed under that fixed name, and the attestation
  name this repository's release workflow.

- [ ] **Read the README's Install section and `wiki/Installing.md` once
  more against the release page.** They were written before a release
  existed and worded to be true on this day. *Not done.*
- [ ] **The Homebrew formula** in `packaging/homebrew/` is a template; a
  tap is a later step and the README says "to follow" until it exists.
