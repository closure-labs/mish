# Automatic pull request queueing

Main requires pull requests, both native Checks jobs from GitHub Actions and
an ALLGREEN merge queue. The active repository ruleset is `24893052`; its
configuration is recorded in `.github/merge-queue-rules.json`. One queue group
builds at a time, and its combined commit must pass the same x86 and ARM
checks before GitHub merges it. There are no bypass actors.

The Merge queue workflow runs trusted `main` automation after a successful PR
Checks run. It admits open, non-draft PRs from this repository's branches,
including Dependabot, only after both current-head native jobs pass. Stale
heads, conflicts, missing/failed checks and required-review blockers wait.
External fork admission remains an explicit maintainer action. Turning a
draft into a ready PR triggers Checks again.

Enqueue uses the expected PR head SHA so a new push cannot silently substitute
untested content. The workflow never checks out PR code or downloads PR
artifacts with its write token. It checks out main with persisted credentials
disabled. Write permissions are limited to the queue job: pull requests and
contents for queue admission, and Actions for CI dispatch.

GitHub can suppress downstream events caused by `GITHUB_TOKEN`. After queue
admission, automation finds the queue entry's head commit and matching
`gh-readonly-queue/main/` ref. It explicitly dispatches Checks on that ref only
when no merge-group or dispatch run already exists. No personal access token
or additional repository secret is needed.

For an already-passing PR waiting on a review or a previously removed queue
entry, dispatch Merge queue after resolving the blocker. A failed combined
queue build needs investigation before requeueing; the workflow does not
continually retry it on a schedule.

```bash
nix develop --command gh workflow run merge-queue.yml --repo closure-labs/mish --ref main
```

To inspect eligibility without enqueuing anything from a trusted checkout:

```bash
nix develop --command env GITHUB_REPOSITORY=closure-labs/mish python .github/scripts/queue_prs.py --dry-run
```

Queue results do not publish releases. Tagged release automation continues to
produce drafts; image builds and physical acceptance remain distinct gates.
