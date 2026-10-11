"""Queue validated current PR heads; execute only from the trusted main branch."""
from __future__ import annotations

import argparse
import json
import os
import subprocess
import time


CHECKS = {'checks (ubuntu-24.04, x86_64-linux)',
          'checks (ubuntu-24.04-arm, aarch64-linux)', 'Software qualification'}
DETAILS = '''query($owner:String!,$repo:String!,$number:Int!) {
  repository(owner:$owner,name:$repo) { pullRequest(number:$number) {
    id headRefOid isMergeQueueEnabled mergeQueueEntry { id position headCommit { oid } }
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


def validate_app_configuration(environment):
    app_id = environment.get('MISH_QUEUE_APP_ID', '')
    if not app_id.isdecimal() or int(app_id) <= 0 or not environment.get('MISH_QUEUE_APP_PRIVATE_KEY', '').strip():
        raise RuntimeError('Configure MISH_QUEUE_APP_ID repository variable and MISH_QUEUE_APP_PRIVATE_KEY secret; see docs/merge-queue.md')


def ensure_native_queue_checks(repository, number):
    """Observe GitHub's native merge_group run; never substitute a dispatch."""
    for _ in range(20):
        entry = details(repository, number)['mergeQueueEntry']
        if entry is None:
            return  # Already merged or removed; do not re-enqueue it here.
        if entry.get('position', 1) > 1:
            print(f'PR #{number}: waiting for preceding queue entries')
            return
        head = entry.get('headCommit')
        if head:
            runs = gh('api', f'repos/{repository}/actions/workflows/ci.yml/runs?event=merge_group&head_sha={head["oid"]}&per_page=100')
            if any(run['event'] == 'merge_group' and run['head_sha'] == head['oid']
                   for run in runs['workflow_runs']):
                print(f'PR #{number}: native merge_group Checks started')
                return
        time.sleep(3)
    raise RuntimeError(f'PR #{number}: native merge_group Checks did not start; verify GitHub App installation and the merge_group trigger (docs/merge-queue.md)')


def queue_ready(repository, dry_run=False):
    if not dry_run and os.environ.get('GITHUB_ACTIONS') == 'true' and (
            os.environ.get('MISH_QUEUE_AUTH') != 'github-app' or not os.environ.get('GH_TOKEN')):
        raise RuntimeError('Automated queue admission requires the configured GitHub App token; see docs/merge-queue.md')
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
                ensure_native_queue_checks(repository, number)
            continue
        if not eligible(pr, queue, repository):
            print(f'PR #{number}: waiting for current-head checks and branch requirements')
            continue
        if not dry_run:
            gh('api', 'graphql', '-f', 'query=' + ENQUEUE,
               '-f', 'id=' + queue['id'], '-f', 'head=' + pr['headRefOid'])
            ensure_native_queue_checks(repository, number)
        queued.append(number)
        print(f'PR #{number}: {"eligible" if dry_run else "queued"} at {pr["headRefOid"]}')
    return queued


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--dry-run', action='store_true')
    parser.add_argument('--check-configuration', action='store_true')
    arguments = parser.parse_args()
    try:
        if arguments.check_configuration:
            validate_app_configuration(os.environ)
        else:
            queue_ready(os.environ['GITHUB_REPOSITORY'], arguments.dry_run)
    except RuntimeError as error:
        parser.exit(1, str(error) + '\n')
