from __future__ import annotations

import copy
import importlib.util
import os
from pathlib import Path
import unittest
from unittest.mock import patch


script = os.environ.get('VESTAFORGE_QUEUE_SCRIPT_PATH', str(Path(__file__).parents[1] / '.github/scripts/queue_prs.py'))
spec = importlib.util.spec_from_file_location('queue_prs', script)
queue_prs = importlib.util.module_from_spec(spec)
spec.loader.exec_module(queue_prs)


class QueueTests(unittest.TestCase):
    def setUp(self):
        self.pr = {'state': 'OPEN', 'isDraft': False, 'baseRefName': 'main',
                   'headRepository': {'nameWithOwner': 'closure-labs/mish'},
                   'headRefOid': 'current-head', 'mergeable': 'MERGEABLE', 'reviewDecision': '',
                   'statusCheckRollup': [{'name': name, 'workflowName': 'Checks',
                                         'status': 'COMPLETED', 'conclusion': 'SUCCESS'}
                                        for name in queue_prs.CHECKS]}
        self.queue = {'id': 'PR-node', 'headRefOid': 'current-head',
                      'isMergeQueueEnabled': True, 'mergeQueueEntry': None}

    def test_only_ready_current_heads_with_both_checks_are_eligible(self):
        self.assertTrue(queue_prs.eligible(self.pr, self.queue, 'closure-labs/mish'))
        for field, value in (('isDraft', True), ('state', 'CLOSED'), ('baseRefName', 'other'),
                             ('headRefOid', 'stale-head'), ('mergeable', 'CONFLICTING'),
                             ('reviewDecision', 'REVIEW_REQUIRED'), ('reviewDecision', 'CHANGES_REQUESTED'),
                             ('headRepository', {'nameWithOwner': 'outside/fork'}), ('headRepository', None)):
            pr = {**self.pr, field: value}
            self.assertFalse(queue_prs.eligible(pr, self.queue, 'closure-labs/mish'))
        self.assertFalse(queue_prs.eligible(self.pr, {**self.queue, 'isMergeQueueEnabled': False}, 'closure-labs/mish'))
        for field, value in (('conclusion', 'FAILURE'), ('status', 'IN_PROGRESS'), ('workflowName', 'Unrelated')):
            pr = copy.deepcopy(self.pr)
            pr['statusCheckRollup'][0][field] = value
            self.assertFalse(queue_prs.eligible(pr, self.queue, 'closure-labs/mish'))
        self.assertFalse(queue_prs.eligible({**self.pr, 'statusCheckRollup': self.pr['statusCheckRollup'][:1]}, self.queue, 'closure-labs/mish'))
        repeated = copy.deepcopy(self.pr)
        repeated['statusCheckRollup'].append({**repeated['statusCheckRollup'][0], 'conclusion': 'FAILURE'})
        self.assertFalse(queue_prs.eligible(repeated, self.queue, 'closure-labs/mish'))

    def test_enqueue_uses_expected_head_and_no_direct_merge(self):
        with patch.object(queue_prs, 'gh', side_effect=[[{'number': 7}], self.pr, {}]) as gh, \
             patch.object(queue_prs, 'details', return_value=self.queue), \
             patch.object(queue_prs, 'ensure_queue_checks') as ensure:
            self.assertEqual(queue_prs.queue_ready('closure-labs/mish'), [7])
        mutation = gh.call_args_list[-1].args
        self.assertIn('head=current-head', mutation)
        self.assertIn('id=PR-node', mutation)
        self.assertIn('query=' + queue_prs.ENQUEUE, mutation)
        ensure.assert_called_once_with('closure-labs/mish', 7)

    def test_dry_run_and_existing_entry_do_not_enqueue(self):
        for entry, dry_run in ((None, True), ({'id': 'entry'}, False)):
            with patch.object(queue_prs, 'gh', side_effect=[[{'number': 7}], self.pr]) as gh, \
                 patch.object(queue_prs, 'details', return_value={**self.queue, 'mergeQueueEntry': entry}), \
                 patch.object(queue_prs, 'ensure_queue_checks'):
                queue_prs.queue_ready('closure-labs/mish', dry_run)
                self.assertEqual(gh.call_count, 2)

    def test_queue_ci_dispatch_fallback_and_existing_run_deduplication(self):
        entry = {'mergeQueueEntry': {'headCommit': {'oid': 'queue-sha'}}}
        ref = [{'ref': 'refs/heads/gh-readonly-queue/main/pr-7-queue-sha', 'object': {'sha': 'queue-sha'}}]
        for existing in (True, False):
            runs = {'workflow_runs': [{'event': 'merge_group'}] if existing else []}
            responses = [ref, runs] if existing else [ref, runs, runs, None]
            with patch.object(queue_prs, 'details', return_value=entry), \
                 patch.object(queue_prs, 'gh', side_effect=responses) as gh, \
                 patch.object(queue_prs.time, 'sleep'):
                queue_prs.ensure_queue_checks('closure-labs/mish', 7)
            if existing:
                self.assertEqual(gh.call_count, 2)
            else:
                self.assertIn('ref=gh-readonly-queue/main/pr-7-queue-sha', gh.call_args.args)
                self.assertIn('POST', gh.call_args.args)

    def test_removed_entry_never_dispatches_or_requeues(self):
        with patch.object(queue_prs, 'details', return_value={'mergeQueueEntry': None}), \
             patch.object(queue_prs, 'gh') as gh:
            queue_prs.ensure_queue_checks('closure-labs/mish', 7)
            gh.assert_not_called()


if __name__ == '__main__':
    unittest.main()
