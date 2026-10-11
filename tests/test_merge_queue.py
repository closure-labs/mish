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
        environment = patch.dict(os.environ, {'GITHUB_ACTIONS': 'false'})
        environment.start()
        self.addCleanup(environment.stop)
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
             patch.object(queue_prs, 'ensure_native_queue_checks') as ensure:
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
                 patch.object(queue_prs, 'ensure_native_queue_checks'):
                queue_prs.queue_ready('closure-labs/mish', dry_run)
                self.assertEqual(gh.call_count, 2)

    def test_native_run_requires_exact_queue_sha(self):
        entry = {'mergeQueueEntry': {'headCommit': {'oid': 'queue-sha'}}}
        runs = {'workflow_runs': [{'event': 'merge_group', 'head_sha': 'queue-sha'}]}
        with patch.object(queue_prs, 'details', return_value=entry), \
             patch.object(queue_prs, 'gh', return_value=runs) as gh:
            queue_prs.ensure_native_queue_checks('closure-labs/mish', 7)
            self.assertEqual(gh.call_count, 1)

    def test_dispatch_and_wrong_sha_never_substitute_for_native_checks(self):
        entry = {'mergeQueueEntry': {'headCommit': {'oid': 'queue-sha'}}}
        for event, sha in (('workflow_dispatch', 'queue-sha'), ('merge_group', 'wrong-sha')):
            with patch.object(queue_prs, 'details', return_value=entry), \
                 patch.object(queue_prs, 'gh', return_value={'workflow_runs': [{'event': event, 'head_sha': sha}]}) as gh, \
                 patch.object(queue_prs.time, 'sleep'):
                with self.assertRaisesRegex(RuntimeError, 'native merge_group'):
                    queue_prs.ensure_native_queue_checks('closure-labs/mish', 7)
                self.assertTrue(all('POST' not in call.args for call in gh.call_args_list))

    def test_later_queue_entry_waits_without_synthetic_ci(self):
        with patch.object(queue_prs, 'details', return_value={'mergeQueueEntry': {'position': 2}}), \
             patch.object(queue_prs, 'gh') as gh:
            queue_prs.ensure_native_queue_checks('closure-labs/mish', 7)
            gh.assert_not_called()

    def test_automation_requires_app_token_before_any_api_call(self):
        for marker, token in (('', 'built-in-token'), ('github-app', '')):
            with patch.dict(os.environ, {'GITHUB_ACTIONS': 'true', 'MISH_QUEUE_AUTH': marker, 'GH_TOKEN': token}), \
                 patch.object(queue_prs, 'gh') as gh:
                with self.assertRaisesRegex(RuntimeError, 'GitHub App token'):
                    queue_prs.queue_ready('closure-labs/mish')
                gh.assert_not_called()
        with patch.dict(os.environ, {'GITHUB_ACTIONS': 'true', 'MISH_QUEUE_AUTH': 'github-app', 'GH_TOKEN': 'app-token'}), \
             patch.object(queue_prs, 'gh', return_value=[]):
            self.assertEqual(queue_prs.queue_ready('closure-labs/mish'), [])

    def test_configuration_error_never_discloses_private_key(self):
        for app_id, key in (('', 'private-key'), ('bad', 'private-key'), ('0', 'private-key'), ('123', '')):
            with self.assertRaises(RuntimeError) as raised:
                queue_prs.validate_app_configuration({'MISH_QUEUE_APP_ID': app_id, 'MISH_QUEUE_APP_PRIVATE_KEY': key})
            self.assertNotIn('private-key', str(raised.exception))
        queue_prs.validate_app_configuration({'MISH_QUEUE_APP_ID': '123', 'MISH_QUEUE_APP_PRIVATE_KEY': 'key'})

    def test_removed_entry_never_dispatches_or_requeues(self):
        with patch.object(queue_prs, 'details', return_value={'mergeQueueEntry': None}), \
             patch.object(queue_prs, 'gh') as gh:
            queue_prs.ensure_native_queue_checks('closure-labs/mish', 7)
            gh.assert_not_called()


if __name__ == '__main__':
    unittest.main()
