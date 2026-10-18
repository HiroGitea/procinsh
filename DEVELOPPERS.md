# Developer guide

## Releasing

From a checkout of this repository, run one of:

```sh
./scripts/release.sh patch  # 0.1.13 -> 0.1.14
./scripts/release.sh minor  # 0.1.13 -> 0.2.0
./scripts/release.sh major  # 0.1.13 -> 1.0.0
```

The argument is required. The version is calculated from the latest
`origin/main`, independently of your current branch. Requires Python 3.11+,
Git, GitHub CLI (`gh auth login`), permission to push branches and merge PRs,
Cargo authentication for publishing `procinsh` to crates.io, and the local
Node/Rust/BPF build prerequisites described in [README.md](README.md#self-build).
Set `BPFTOOL` if needed.

The script uses a separate worktree to update `Cargo.toml` and `Cargo.lock`,
build the Web assets, and verify the crate with `scripts/check_web_package.py`.
It then creates a version PR, waits for its checks and required reviews, and
merges it using the repository's default merge method. Repository auto-merge
does not need to be enabled. Required reviews still need a reviewer; checks
and branch protection are never bypassed.

After merging, it checks out that PR's exact merge commit, rebuilds and verifies
the crate, runs `cargo publish --locked --allow-dirty --registry crates-io`,
confirms the published archive, and pushes an annotated `v<version>` tag.
`--allow-dirty` permits the generated, ignored `web/dist` assets in the crate;
the script checks that tracked files and other nonignored files are clean.
It leaves your current branch and working files in place. No GitHub Release
or prebuilt binary is created.

Progress and the publication archive are saved under
`$(git rev-parse --git-common-dir)/procinsh-release/`. Only one local release
can run at a time, including across linked worktrees. Use Ctrl-C to pause;
rerun with the **same argument** to resume the saved version and PR without
incrementing again. The script does not automatically roll back merged PRs
or published versions.

If checks fail, fix or rerun them on GitHub and resume. An edited PR head,
conflict, or a requirement to update with main stops the script for inspection;
do not edit the version PR or force a merge while the script is running.
After an interrupted publish, it compares the registry checksum, embedded Git
commit, and archived file contents against the saved attempt before proceeding.
Unexpected preexisting publications or conflicting tags stop the release and
are never overwritten. Keep the saved state and worktree until the issue is
resolved. Successful releases retain a `v<version>.json` record and remove the
temporary worktree; the next invocation starts a new release.

Run the offline release tests with `python3 tests/release.py`. They simulate
GitHub, builds, and crates.io while exercising real temporary Git repositories.
