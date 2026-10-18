#!/usr/bin/env python3
"""Release from origin/main, keeping resumable state in Git's common directory."""

import argparse
import fcntl
import hashlib
import io
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tarfile
import time
import tomllib
import urllib.error
import urllib.request


REPO = "akawashiro/procinsh"
PACKAGE = "procinsh"


def run(*args, cwd, capture=False):
    env = dict(os.environ)
    env.pop("PROCINSH_GIT_SHA", None)
    env["CARGO_TARGET_DIR"] = str(Path(cwd) / "target")
    result = subprocess.run(args, cwd=cwd, env=env, check=True, text=True,
                            stdout=subprocess.PIPE if capture else None)
    return result.stdout.strip() if capture else None


def bump(version, kind):
    if not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", version):
        raise RuntimeError(f"Expected a stable version, got {version!r}")
    parts = list(map(int, version.split(".")))
    index = {"major": 0, "minor": 1, "patch": 2}[kind]
    parts[index] += 1
    parts[index + 1:] = [0] * (2 - index)
    return ".".join(map(str, parts))


def set_versions(root, previous, version):
    """Change only the package's own version, preserving all other formatting."""
    for filename, section in [("Cargo.toml", "package"), ("Cargo.lock", "[package]")]:
        path = root / filename
        original = path.read_text()
        parsed = tomllib.loads(original)
        package = (parsed["package"] if filename == "Cargo.toml" else
                   next(p for p in parsed["package"] if p["name"] == PACKAGE))
        if package["version"] not in (previous, version):
            raise RuntimeError(f"Unexpected {filename} version: {package['version']}")
        chunks = re.split(r"(?m)(?=^\[)", original)
        found = 0
        for index, chunk in enumerate(chunks):
            if not chunk.startswith(f"[{section}]\n"):
                continue
            data = tomllib.loads(chunk)["package"]
            if isinstance(data, list):
                data = data[0]
            if data.get("name") == PACKAGE:
                chunks[index], count = re.subn(
                    r'(?m)^version = "[^"]+"$', f'version = "{version}"', chunk)
                found += count
        if found != 1:
            raise RuntimeError(f"Could not locate exactly one package version in {filename}")
        path.write_text("".join(chunks))


def download(url):
    request = urllib.request.Request(url, headers={"User-Agent": "procinsh-release (github.com/akawashiro/procinsh)"})
    with urllib.request.urlopen(request, timeout=60) as response:
        return response.read()


def published(version):
    try:
        return json.loads(download(f"https://crates.io/api/v1/crates/{PACKAGE}/{version}"))["version"]
    except urllib.error.HTTPError as error:
        if error.code == 404:
            return None
        raise


def archive_files(data):
    with tarfile.open(fileobj=io.BytesIO(data), mode="r:gz") as archive:
        return {member.name: archive.extractfile(member).read()
                for member in archive.getmembers() if member.isfile()}


def verify_published(version, commit, expected, metadata):
    data = download(f"https://static.crates.io/crates/{PACKAGE}/{PACKAGE}-{version}.crate")
    if hashlib.sha256(data).hexdigest() != metadata["checksum"]:
        raise RuntimeError("Downloaded crate does not match the registry checksum")
    files = archive_files(data)
    vcs = json.loads(files[f"{PACKAGE}-{version}/.cargo_vcs_info.json"])
    if vcs["git"]["sha1"] != commit or files != archive_files(expected.read_bytes()):
        raise RuntimeError("Published crate differs from this release attempt; inspect it before continuing")


class Release:
    def __init__(self, root, directory, kind):
        self.root = root
        self.directory = directory
        self.state_path = directory / "current.json"
        self.work = directory / "worktree"
        self.kind = kind
        self.state = json.loads(self.state_path.read_text()) if self.state_path.exists() else {}
        if self.state and self.state["kind"] != kind:
            raise RuntimeError(f"Pending {self.state['version']} release: rerun with {self.state['kind']}")

    def save(self, **values):
        self.state.update(values)
        temporary = self.state_path.with_suffix(".tmp")
        temporary.write_text(json.dumps(self.state, indent=2) + "\n")
        temporary.replace(self.state_path)

    def git(self, *args, work=False):
        return run("git", *args, cwd=self.work if work else self.root, capture=True)

    def gh(self, *args):
        return run("gh", *args, "--repo", REPO, cwd=self.root, capture=True)

    def ensure_clean(self):
        if self.git("status", "--porcelain", "--untracked-files=all", work=True):
            raise RuntimeError(f"Unexpected files in release worktree {self.work}; inspect before resuming")

    def build(self, expected):
        self.ensure_clean()
        if self.git("rev-parse", "HEAD", work=True) != expected:
            raise RuntimeError("Release worktree is at an unexpected commit")
        # Remove ignored output too, so a previous build cannot contribute assets.
        shutil.rmtree(self.work / "web/dist", ignore_errors=True)
        for command in [("npm", "--prefix", "web", "ci"),
                        ("npm", "--prefix", "web", "run", "build"),
                        ("python3", "scripts/check_web_package.py")]:
            run(*command, cwd=self.work)
        self.ensure_clean()
        if self.git("rev-parse", "HEAD", work=True) != expected:
            raise RuntimeError("Build changed the release worktree commit")

    def prepare(self):
        if not self.state:
            self.git("fetch", "origin", "main", "--tags")
            base = self.git("rev-parse", "origin/main")
            manifest = tomllib.loads(self.git("show", f"{base}:Cargo.toml"))
            previous = manifest["package"]["version"]
            version = bump(previous, self.kind)
            if published(version):
                raise RuntimeError(f"{version} is already published; reconcile origin/main first")
            if self.git("tag", "--list", f"v{version}"):
                raise RuntimeError(f"Tag v{version} already exists")
            self.save(kind=self.kind, previous=previous, version=version, base=base,
                      branch=f"release/v{version}")
        print(f"Releasing {self.state['version']} ({self.kind}); state: {self.state_path}", flush=True)
        if not self.work.exists():
            if "head" in self.state or "commit" in self.state:
                revision = self.state.get("commit", self.state.get("head"))
            else:
                revision = self.state["base"]
            self.git("worktree", "add", "--detach", str(self.work), revision)

        if "head" not in self.state:
            # Recover a commit made just before an interruption while saving state.
            head = self.git("rev-parse", "HEAD", work=True)
            if head != self.state["base"]:
                self.validate_version_commit(head)
            else:
                changed = self.git("diff", "--name-only", "HEAD", work=True).splitlines()
                untracked = self.git("ls-files", "--others", "--exclude-standard", work=True)
                if set(changed) - {"Cargo.toml", "Cargo.lock"} or untracked:
                    raise RuntimeError("Unexpected changes in release worktree")
                set_versions(self.work, self.state["previous"], self.state["version"])
                self.git("add", "Cargo.toml", "Cargo.lock", work=True)
                self.git("commit", "-m", f"Release v{self.state['version']}", work=True)
                head = self.git("rev-parse", "HEAD", work=True)
                self.validate_version_commit(head)
            self.save(head=head)

        if "pr" not in self.state:
            self.build(self.state["head"])
            self.git("push", "origin", f"{self.state['head']}:refs/heads/{self.state['branch']}")
            prs = json.loads(self.gh("pr", "list", "--head", self.state["branch"],
                                     "--base", "main", "--state", "all", "--json", "url"))
            if len(prs) > 1:
                raise RuntimeError("Multiple release PRs found; inspect before resuming")
            if prs:
                url = prs[0]["url"]
            else:
                body = self.directory / "pr-body.md"
                body.write_text(f"Release procinsh {self.state['version']}.\n\n"
                                "Updates the package version and lockfile. Local Web build and "
                                "crate packaging verification passed before PR creation.\n")
                url = self.gh("pr", "create", "--base", "main", "--head", self.state["branch"],
                              "--title", f"Release v{self.state['version']}", "--body-file", str(body))
            self.save(pr=url)

    def validate_version_commit(self, head):
        if self.git("rev-parse", f"{head}^", work=True) != self.state["base"]:
            raise RuntimeError("Unexpected release commit ancestry")
        changed = set(self.git("diff", "--name-only", self.state["base"], head, work=True).splitlines())
        if changed != {"Cargo.toml", "Cargo.lock"}:
            raise RuntimeError("Release commit must change only Cargo.toml and Cargo.lock")
        # Compare against the exact expected patch, not just its file names.
        for filename in changed:
            expected = self.git("show", f"{self.state['base']}:{filename}", work=True)
            actual = self.git("show", f"{head}:{filename}", work=True)
            pattern = rf'(?m)^(name = "{PACKAGE}"\nversion = "){re.escape(self.state["previous"])}("$)'
            expected, count = re.subn(pattern, rf'\g<1>{self.state["version"]}\2', expected)
            if count != 1 or expected != actual:
                raise RuntimeError(f"Unexpected release changes in {filename}")

    def merge(self):
        if "commit" in self.state:
            return
        fields = "state,headRefOid,baseRefName,mergeCommit,mergeable,mergeStateStatus,reviewDecision,statusCheckRollup"
        requested = False
        while True:
            pr = json.loads(self.gh("pr", "view", self.state["pr"], "--json", fields))
            if pr["headRefOid"] != self.state["head"] or pr["baseRefName"] != "main":
                raise RuntimeError("Release PR head or base changed; inspect before resuming")
            if pr["state"] == "MERGED":
                self.save(commit=pr["mergeCommit"]["oid"])
                break
            if pr["state"] == "CLOSED":
                raise RuntimeError("Release PR was closed without merging")
            if pr["mergeable"] == "CONFLICTING":
                raise RuntimeError("Release PR conflicts with main; resolve before resuming")
            checks = pr["statusCheckRollup"] or []
            failed = {"FAILURE", "CANCELLED", "TIMED_OUT", "ACTION_REQUIRED", "STARTUP_FAILURE", "ERROR"}
            if any((c.get("conclusion") or c.get("state")) in failed for c in checks):
                raise RuntimeError(f"PR checks failed: {self.state['pr']}; fix/rerun checks, then resume")
            passed = checks and all(c.get("conclusion") in {"SUCCESS", "NEUTRAL", "SKIPPED"}
                                    or c.get("state") == "SUCCESS" for c in checks)
            if passed and not requested:
                settings = json.loads(run("gh", "repo", "view", REPO, "--json", "viewerDefaultMergeMethod",
                                          cwd=self.root, capture=True))
                method = str(settings["viewerDefaultMergeMethod"]).lower()
                if method not in {"merge", "squash", "rebase"}:
                    raise RuntimeError(f"Unsupported default merge method: {method}")
                if pr["mergeStateStatus"] == "BEHIND":
                    raise RuntimeError("Release PR must be updated with main; inspect before resuming")
                # Poll approvals locally so repository auto-merge need not be enabled.
                # gh pr merge also enqueues a ready PR when a merge queue is required.
                if (pr["reviewDecision"] not in {"REVIEW_REQUIRED", "CHANGES_REQUESTED"}
                        and pr["mergeStateStatus"] not in {"BLOCKED", "UNKNOWN", "DRAFT"}):
                    self.gh("pr", "merge", self.state["pr"], f"--{method}",
                            "--match-head-commit", self.state["head"])
                    requested = True
            print(f"Waiting for PR checks/approval/merge: {self.state['pr']} (Ctrl-C to pause)", flush=True)
            time.sleep(10)

    def publish(self):
        version = self.state["version"]
        commit = self.state["commit"]
        attempt = self.directory / "attempt.crate"
        if "attempt" in self.state:
            if not attempt.exists() or hashlib.sha256(attempt.read_bytes()).hexdigest() != self.state["attempt"]:
                raise RuntimeError("Saved publication archive is missing or changed; inspect state")
        metadata = published(version)
        if metadata:
            if "attempt" not in self.state:
                raise RuntimeError("Version was published outside this attempt; inspect before continuing")
            verify_published(version, commit, attempt, metadata)
            self.save(published=True)
            return
        if self.state.get("published"):
            raise RuntimeError("Previously confirmed release is missing from crates.io; retry later")
        self.git("fetch", "origin", "main")
        self.git("merge-base", "--is-ancestor", commit, "origin/main")
        self.ensure_clean()
        self.git("checkout", "--detach", commit, work=True)
        self.build(commit)
        self.ensure_clean()
        manifest = tomllib.loads((self.work / "Cargo.toml").read_text())
        if manifest["package"]["version"] != version:
            raise RuntimeError("Merged commit has the wrong version")
        archive = self.work / f"target/package/{PACKAGE}-{version}.crate"
        vcs = json.loads(archive_files(archive.read_bytes())[f"{PACKAGE}-{version}/.cargo_vcs_info.json"])
        if vcs["git"]["sha1"] != commit:
            raise RuntimeError("Packaged crate has the wrong Git commit; refusing to publish")
        if "attempt" in self.state:
            if archive_files(attempt.read_bytes()) != archive_files(archive.read_bytes()):
                raise RuntimeError("Rebuilt crate differs from saved publication attempt; inspect before retrying")
        else:
            shutil.copyfile(archive, attempt)
            self.save(attempt=hashlib.sha256(attempt.read_bytes()).hexdigest())
        print(f"Publishing {version} from {commit}", flush=True)
        run("cargo", "publish", "--locked", "--allow-dirty", "--registry", "crates-io", cwd=self.work)
        # Cargo can time out after uploading. An interrupted/failed run is resumed
        # through the same registry/archive verification above.
        metadata = published(version)
        if not metadata:
            raise RuntimeError("Upload finished but registry confirmation is pending; rerun to resume")
        verify_published(version, commit, attempt, metadata)
        self.save(published=True)

    def tag(self):
        tag = f"v{self.state['version']}"
        commit = self.state["commit"]
        local = self.git("tag", "--list", tag)
        if local and self.git("rev-parse", f"{tag}^{{commit}}") != commit:
            raise RuntimeError(f"Local tag {tag} points to another commit; it will not be overwritten")
        remote = self.git("ls-remote", "--tags", "origin", f"refs/tags/{tag}", f"refs/tags/{tag}^{{}}")
        if remote:
            refs = dict(line.split()[::-1] for line in remote.splitlines())
            if refs.get(f"refs/tags/{tag}^{{}}", refs.get(f"refs/tags/{tag}")) != commit:
                raise RuntimeError(f"Remote tag {tag} points to another commit; it will not be overwritten")
        else:
            if not local:
                self.git("tag", "-a", tag, commit, "-m", f"Release {tag}")
            self.git("push", "origin", f"refs/tags/{tag}")
        self.save(complete=True)
        if self.work.exists():
            self.ensure_clean()
            self.git("worktree", "remove", str(self.work))
        history = self.directory / f"{tag}.json"
        self.state_path.replace(history)
        print(f"Released {tag}: https://crates.io/crates/{PACKAGE}/{self.state['version']}", flush=True)

    def execute(self):
        if self.state.get("complete"):
            self.tag()
            return
        self.prepare()
        self.merge()
        self.publish()
        self.tag()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("kind", choices=["patch", "minor", "major"])
    args = parser.parse_args()
    root = Path(run("git", "rev-parse", "--show-toplevel", cwd=Path(__file__).resolve().parent, capture=True))
    common = Path(run("git", "rev-parse", "--path-format=absolute", "--git-common-dir", cwd=root, capture=True))
    directory = common / "procinsh-release"
    directory.mkdir(exist_ok=True)
    with (directory / "lock").open("w") as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            parser.exit(1, "Another release is running for this repository.\n")
        try:
            for tool in ["git", "gh", "npm", "cargo"]:
                if not shutil.which(tool):
                    raise RuntimeError(f"Required executable not found: {tool}")
            run("gh", "auth", "status", cwd=root)
            origin = run("git", "remote", "get-url", "--push", "origin", cwd=root, capture=True)
            if not re.fullmatch(r"(?:ssh://git@github\.com/|git@github\.com:|https://github\.com/)"
                                + re.escape(REPO) + r"(?:\.git)?", origin):
                raise RuntimeError(f"origin must push to github.com/{REPO}, got {origin!r}")
            Release(root, directory, args.kind).execute()
        except (RuntimeError, subprocess.CalledProcessError, OSError, ValueError, KeyError, tarfile.TarError) as error:
            parser.exit(1, f"Release stopped: {error}\nState: {directory / 'current.json'}\n"
                        f"After resolving the cause, rerun ./scripts/release.sh {args.kind}.\n")
        except KeyboardInterrupt:
            parser.exit(130, f"\nPaused. Rerun ./scripts/release.sh {args.kind} to resume.\n")


if __name__ == "__main__":
    main()
