"""Queue validated current PR heads; execute only from the trusted main branch."""
from __future__ import annotations

import argparse
import json
import os
import subprocess
import time


CHECKS = {'checks (ubuntu-24.04, x86_64-linux)',
          'checks (ubuntu-24.04-arm, aarch64-linux)'}
DETAILS = '''query($owner:String!,$repo:String!,$number:Int!) {
  repository(owner:$owner,name:$repo) { pullRequest(number:$number) {
    id headRefOid isMergeQueueEnabled mergeQueueEntry { id headCommit { oid } }
  } }
}'''
ENQUEUE = '''mutation($id:ID!,$head:GitObjectID!) {
  enqueuePullRequest(input:{pullRequestId:$id,expectedHeadOid:$head}) {
    mergeQueueEntry { id }
  }
}'''


def gh(*arguments):
    result = subprocess.run(['gh', *arguments], check=True, capture_output=True, text=True)
    return json.loads(result.stdout) if result.stdout.strip() else None


def details(repository, number):
    owner, repo = repository.split('/')
    result = gh('api', 'graphql', '-f', 'query=' + DETAILS,
                '-f', 'owner=' + owner, '-f', 'repo=' + repo, '-F', 'number=' + str(number))
    return result['data']['repository']['pullRequest']


def eligible(pr, queue, repository):
    if pr['state'] != 'OPEN' or pr['isDraft'] or pr['baseRefName'] != 'main':
        return False
    if (pr.get('headRepository') or {}).get('nameWithOwner') != repository:
        return False  # Outside contributors require a maintainer queue decision.
    if not queue['isMergeQueueEnabled'] or pr['headRefOid'] != queue['headRefOid']:
        return False
    if pr['mergeable'] != 'MERGEABLE' or pr['reviewDecision'] in ('REVIEW_REQUIRED', 'CHANGES_REQUESTED'):
        return False
    checks = [check for check in pr['statusCheckRollup']
              if check.get('workflowName') == 'Checks' and check['name'] in CHECKS]
    return CHECKS <= {check['name'] for check in checks} and all(
        check.get('status') == 'COMPLETED' and check.get('conclusion') == 'SUCCESS'
        for check in checks)


def ensure_queue_checks(repository, number):
    """GITHUB_TOKEN events can suppress CI; dispatch only if queue CI is absent."""
    for _ in range(10):
        entry = details(repository, number)['mergeQueueEntry']
        if entry is None:
            return  # Already merged or removed; do not re-enqueue it here.
        head = entry.get('headCommit')
        if head:
            refs = gh('api', f'repos/{repository}/git/matching-refs/heads/gh-readonly-queue/main/')
            matching = [ref['ref'][len('refs/heads/'):] for ref in refs if ref['object']['sha'] == head['oid']]
            if matching:
                runs = gh('api', f'repos/{repository}/actions/workflows/ci.yml/runs?head_sha={head["oid"]}&per_page=100')
                if any(run['event'] in ('merge_group', 'workflow_dispatch') for run in runs['workflow_runs']):
                    return
                # Give the native merge_group event time to create its run first.
                time.sleep(3)
                runs = gh('api', f'repos/{repository}/actions/workflows/ci.yml/runs?head_sha={head["oid"]}&per_page=100')
                if not any(run['event'] in ('merge_group', 'workflow_dispatch') for run in runs['workflow_runs']):
                    gh('api', '--method', 'POST', f'repos/{repository}/actions/workflows/ci.yml/dispatches', '-f', 'ref=' + matching[0])
                    print(f'PR #{number}: dispatched Checks for its queue commit')
                return
        time.sleep(3)
    raise RuntimeError(f'PR #{number}: queue commit was not available for CI; rerun the queue workflow')


def queue_ready(repository, dry_run=False):
    candidates = gh('pr', 'list', '--repo', repository, '--base', 'main', '--state', 'open',
                    '--limit', '1000', '--json', 'number')
    queued = []
    for candidate in candidates:
        number = candidate['number']
        pr = gh('pr', 'view', str(number), '--repo', repository, '--json',
                'state,isDraft,baseRefName,headRefOid,headRepository,mergeable,reviewDecision,statusCheckRollup')
        queue = details(repository, number)
        if queue['mergeQueueEntry']:
            print(f'PR #{number}: already queued')
            if not dry_run:
                ensure_queue_checks(repository, number)
            continue
        if not eligible(pr, queue, repository):
            print(f'PR #{number}: waiting for current-head checks and branch requirements')
            continue
        if not dry_run:
            gh('api', 'graphql', '-f', 'query=' + ENQUEUE,
               '-f', 'id=' + queue['id'], '-f', 'head=' + pr['headRefOid'])
            ensure_queue_checks(repository, number)
        queued.append(number)
        print(f'PR #{number}: {"eligible" if dry_run else "queued"} at {pr["headRefOid"]}')
    return queued


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--dry-run', action='store_true')
    arguments = parser.parse_args()
    queue_ready(os.environ['GITHUB_REPOSITORY'], arguments.dry_run)
