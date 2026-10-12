# Native pull request queueing

Main requires pull requests, both native Checks jobs and Software qualification from GitHub Actions and
an ALLGREEN merge queue. The active repository ruleset is `24893052`; its
configuration is recorded in `.github/merge-queue-rules.json`. One queue group
builds at a time, and its combined commit must pass the same x86 and ARM
checks before GitHub merges it. There are no bypass actors.

## Current path: per-PR auto-merge

The owner confirmed the current ruleset and deferred App setup. Use GitHub's
native queue with an authenticated maintainer session. Enable auto-merge for
each reviewed PR; GitHub waits for the required PR checks, admits it to the
queue and runs native merge-group validation before merging.

For example, replace `24` and `FULL_PR_HEAD_SHA` with the intended PR and its
current full commit SHA:

```bash
nix develop --command gh pr merge 24 --auto --match-head-commit FULL_PR_HEAD_SHA
```

The expected head protects against admitting a changed revision. Failed checks,
conflicts and review blockers still wait. No App or workflow credential setup
is required for this maintainer-driven path. Native auto-merge was exercised
by [PR #24](https://github.com/closure-labs/mish/pull/24) with all three required
merge-group contexts passing. This does not automatically enable auto-merge
on every newly opened PR.

## Optional future path: dispatched App admission

The App workflow is disabled unless repository variable
`MISH_QUEUE_APP_AUTOMATION` is exactly `true`. Leave it unset for the current
path. The **Optional App queue admission** workflow runs only when explicitly
dispatched. Completing Checks does not trigger it. A manual dispatch without
the opt-in variable skips admission and does not request App credentials.

When explicitly enabled and dispatched, it runs trusted `main` automation.
It admits open, non-draft PRs from this repository's branches,
including Dependabot, only after both current-head native jobs and the aggregate software gate pass. Stale
heads, conflicts, missing/failed checks and required-review blockers wait.
External fork admission remains an explicit maintainer action. Turning a
draft into a ready PR triggers Checks again.
Maintenance input updates are created as drafts so the operator reviews them
before marking them ready for Checks and queue admission.

Enqueue uses the expected PR head SHA so a new push cannot silently substitute
untested content. The workflow never checks out PR code or downloads PR
artifacts with its write token. It checks out main with persisted credentials
disabled. The built-in `GITHUB_TOKEN` has read-only contents access. A separate
GitHub App installation token has repository-scoped queue admission permissions,
and read access to Actions and checks. It cannot write check results or bypass
the ruleset. The action revokes its short-lived token when the job finishes.

GitHub suppresses downstream merge-group events caused by `GITHUB_TOKEN`.
Queue admission therefore uses an installation token from a GitHub App, so
GitHub emits the native `merge_group` event consumed by Checks. Automation
observes that event's run on the exact queue SHA; it never substitutes a manual
dispatch or writes synthetic successful checks. Later queue entries wait for
the preceding group. Missing credentials fail before queue admission, and a
missing native event produces an actionable failure after a bounded wait.

Configure the App before using this optional admission helper:

1. An organization owner registers a private GitHub App for `closure-labs`.
   `.github/merge-queue-app-manifest.json` records the registration settings:
   no webhook or subscriptions; repository Actions and Checks read access;
   Contents, Pull requests, and Merge queues write access. No organization
   permissions, administration access, status writes, or bypass grant are needed.
   Choose an unused App name if the manifest name is taken.
2. Install the App on **only** `closure-labs/mish`. Generate a private key in
   the App settings. Keep the downloaded key outside the checkout and Nix store.
3. Record the numeric App ID (not installation ID) in repository variable
   `MISH_QUEUE_APP_ID`, and upload the PEM as repository Actions secret
   `MISH_QUEUE_APP_PRIVATE_KEY`. For example, using an already-authorized local
   GitHub session, replace the numeric ID and private key path:

   ```bash
   nix develop --command gh variable set MISH_QUEUE_APP_ID --repo closure-labs/mish --body 123456
   nix develop --command gh secret set MISH_QUEUE_APP_PRIVATE_KEY --repo closure-labs/mish < /secure/path/queue-app.pem
   ```

4. Set repository variable `MISH_QUEUE_APP_AUTOMATION` to `true` only when
   App admission is wanted and the App credentials are configured.
   Dispatch Optional App queue admission and verify a native `merge_group` Checks run appears
   on the queue commit. Both required architecture jobs must pass before the
   queue merges it. An entry originally admitted with `GITHUB_TOKEN` may need
   a maintainer to remove it from the queue and admit it again with App or
   existing local authentication. Investigate failed combined checks first.

The workflow requests the same limited permissions explicitly when minting
the token, scoped to the repository by owner and repository name. Rotate the
private key by replacing the secret and deleting the old key after validating
the new token. Neither an App nor its credentials is created automatically;
registration, installation, credentials and explicit opt-in are prerequisites
only for this optional dispatched path.
No personal access token is stored by this workflow.

This follows GitHub's [workflow triggering rules](https://docs.github.com/en/actions/how-tos/write-workflows/choose-when-workflows-run/trigger-a-workflow)
and [GitHub App workflow authentication](https://docs.github.com/en/apps/creating-github-apps/authenticating-with-a-github-app/making-authenticated-api-requests-with-a-github-app-in-a-github-actions-workflow).
GitHub's [merge queue documentation](https://docs.github.com/en/enterprise-cloud@latest/repositories/configuring-branches-and-merges-in-your-repository/configuring-pull-request-merges/managing-a-merge-queue)
requires checks triggered by the native merge group event.

For the optional enabled App path, an already-passing PR waiting on a review or
a previously removed queue entry can use workflow dispatch after resolving the
blocker. A failed combined
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

## Understanding skipped runs

Required queue validation appears under **Checks** with event `merge_group`.
Its x86, ARM and **Software qualification** jobs must all succeed. The
aggregate gate rejects skipped, cancelled, failed or missing prerequisites.
The optional admission helper is not a required check and does not validate
the combined queue commit.

Earlier **Merge queue** runs automatically triggered after Checks and skipped
the App job while setup was deferred. Those historical skipped runs remain
visible. For example, [run 38200927727](https://github.com/closure-labs/mish/actions/runs/38200927727)
skipped optional admission, while the actual
[merge-group run 38200125296](https://github.com/closure-labs/mish/actions/runs/38200125296)
passed all required jobs and merged PR #25. The helper now has a distinct name
and no automatic trigger, so routine validation produces no admission runs.
