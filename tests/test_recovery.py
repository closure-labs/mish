from __future__ import annotations

import contextlib
import io
import json
from pathlib import Path
import sqlite3
import tempfile
import time
import unittest
from unittest.mock import patch

from vestaforge.appliance import EventStore
from vestaforge.cli import main
from vestaforge.radio import RadioWorker


class RecoveryTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.root = Path(self.directory.name)
        self.store = EventStore(self.root / 'original')
        self.operator = self.store.enroll('Operator', 'operator', first=True)
        self.staff = self.store.enroll('Staff')
        self.backup = self.root / 'backup.sqlite'
        self.spare = self.root / 'spare'

    def tearDown(self):
        self.directory.cleanup()

    def restore(self, source=None, destination=None):
        output = io.StringIO()
        errors = io.StringIO()
        with contextlib.redirect_stdout(output), contextlib.redirect_stderr(errors):
            result = main(['--state-dir', str(destination or self.spare), 'restore',
                           '--input', str(source or self.backup)])
        return result, output.getvalue(), errors.getvalue()

    def test_snapshot_restores_private_and_public_history_without_radio_replay(self):
        queued = self.store.create('Queued at backup time', self.operator['id'])
        sending = self.store.create('Interrupted send', self.operator['id'])
        self.store.submission(sending['id'], 'sending')
        alert = self.store.create('Gate alert', self.operator['id'], 'urgent',
                                  self.staff['id'], time.time() + 3600)
        self.store.submission(alert['id'], 'sending')
        self.store.submission(alert['id'], 'submitted', 'packet-alert')
        self.store.transport_confirmation('packet-alert')
        self.store.ack(alert['id'], self.staff['id'])
        self.store.receive('radio-ack', '!radio', 2, 'ACK ' + alert['id'])
        announcement = self.store.publish('Water station open')
        self.store.backup(self.backup)
        source_bytes = self.backup.read_bytes()
        # The original sends this after the snapshot. A spare cannot know that.
        self.store.submission(queued['id'], 'sending')
        self.store.submission(queued['id'], 'submitted', 'packet-later')

        result, output, errors = self.restore()
        self.assertEqual(result, 0, errors)
        report = json.loads(output)
        self.assertEqual(report['quarantined'], 2)
        self.assertEqual(len(report['sourceSha256']), 64)
        restored = EventStore(self.spare)
        self.assertEqual(restored.authenticate(self.staff['token'])['id'], self.staff['id'])
        for item in (queued, sending):
            record = restored.record(item['id'])
            self.assertEqual(record['state'], 'uncertain')
            self.assertIsNotNone(record['delivery']['uncertainAt'])
            self.assertIsNone(record['delivery']['radioSubmittedAt'])
        restored_alert = restored.record(alert['id'])
        self.assertEqual(restored_alert['state'], 'confirmed')
        self.assertIsNotNone(restored_alert['delivery']['radioSubmittedAt'])
        self.assertIsNotNone(restored_alert['delivery']['transportConfirmedAt'])
        self.assertEqual({ack['source'] for ack in restored_alert['acknowledgements']}, {'browser', 'radio'})
        self.assertEqual(restored.public(), [announcement])
        self.assertNotIn(self.staff['id'], json.dumps(restored.public()))
        with restored.connect() as db:
            history = dict(db.execute('SELECT * FROM recovery_history').fetchone())
        self.assertEqual(history['source_sha256'], report['sourceSha256'])
        self.assertEqual(history['quarantined'], 2)
        worker = RadioWorker(restored, {'radio': {'adapter': 'simulator'}})
        worker.tick(0)
        self.assertEqual(worker.transport.sent, [])
        self.assertEqual(self.backup.read_bytes(), source_bytes)
        self.assertEqual((self.spare / 'events.sqlite').stat().st_mode & 0o777, 0o600)
        # A subsequent ordinary restart still sends newly accepted messages.
        fresh = restored.create('New message after recovery', self.staff['id'])
        restarted = EventStore(self.spare)
        next_worker = RadioWorker(restarted, {'radio': {'adapter': 'simulator'}})
        next_worker.tick(0)
        self.assertEqual(restarted.record(fresh['id'])['state'], 'submitted')
        self.assertEqual(len(next_worker.transport.sent), 1)

    def test_existing_database_and_sidecars_are_never_overwritten(self):
        self.store.backup(self.backup)
        for name in ('events.sqlite', 'events.sqlite-wal', 'events.sqlite-shm'):
            destination = self.root / name.replace('.', '-')
            destination.mkdir()
            existing = destination / name
            existing.write_bytes(b'keep this state')
            result, output, errors = self.restore(destination=destination)
            self.assertEqual(result, 2)
            self.assertEqual(output, '')
            self.assertIn('existing event state', errors)
            self.assertEqual(existing.read_bytes(), b'keep this state')
            self.assertEqual(list(destination.iterdir()), [existing])

    def test_corrupt_missing_and_incompatible_inputs_leave_no_database(self):
        corrupt = self.root / 'corrupt.sqlite'
        corrupt.write_bytes(b'not a database')
        unrelated = self.root / 'unrelated.sqlite'
        with sqlite3.connect(unrelated) as db:
            db.execute('CREATE TABLE other(value TEXT)')
        for source in (corrupt, unrelated, self.root / 'missing.sqlite'):
            result, output, errors = self.restore(source=source)
            self.assertEqual(result, 2)
            self.assertEqual(output, '')
            self.assertIn('vfctl:', errors)
            self.assertFalse((self.spare / 'events.sqlite').exists())
            self.assertFalse(list(self.spare.glob('.restore-*')))

    def test_live_snapshot_sidecars_are_rejected(self):
        self.store.backup(self.backup)
        sidecar = Path(str(self.backup) + '-wal')
        sidecar.write_bytes(b'pending WAL')
        result, _, errors = self.restore()
        self.assertEqual(result, 2)
        self.assertIn('standalone backup', errors)
        self.assertFalse((self.spare / 'events.sqlite').exists())

    def test_incompatible_schema_and_triggers_are_rejected(self):
        self.store.backup(self.backup)
        for change in (
            'ALTER TABLE records DROP COLUMN uncertain_at',
            'ALTER TABLE staff ADD COLUMN unexpected TEXT',
            'ALTER TABLE staff ADD COLUMN generated TEXT GENERATED ALWAYS AS (name) VIRTUAL',
            '''ALTER TABLE staff RENAME TO old_staff;
               CREATE TABLE staff(name TEXT,id TEXT PRIMARY KEY,role TEXT,hash TEXT UNIQUE);
               INSERT INTO staff SELECT name,id,role,hash FROM old_staff;
               DROP TABLE old_staff;''',
            '''ALTER TABLE received RENAME TO old_received;
               CREATE TABLE received(packet_id TEXT,node TEXT,channel INTEGER,created REAL);
               INSERT INTO received SELECT * FROM old_received;
               DROP TABLE old_received;''',
            "CREATE TRIGGER replay AFTER UPDATE ON records BEGIN UPDATE records SET state='queued'; END",
        ):
            modified = self.root / 'modified.sqlite'
            modified.write_bytes(self.backup.read_bytes())
            with sqlite3.connect(modified) as db:
                db.execute('PRAGMA journal_mode=DELETE')
                db.executescript(change)
            result, _, _ = self.restore(source=modified)
            self.assertEqual(result, 2)
            self.assertFalse((self.spare / 'events.sqlite').exists())
            self.assertFalse(list(self.spare.glob('.restore-*')))

    def test_concurrent_database_creation_is_preserved(self):
        self.store.backup(self.backup)
        import os
        real_link = os.link
        def race(source, destination):
            Path(destination).write_bytes(b'concurrent commissioning')
            return real_link(source, destination)
        with patch('vestaforge.recovery.os.link', side_effect=race):
            result, output, _ = self.restore()
        self.assertEqual(result, 2)
        self.assertEqual(output, '')
        self.assertEqual((self.spare / 'events.sqlite').read_bytes(), b'concurrent commissioning')
        self.assertFalse(list(self.spare.glob('.restore-*')))

    def test_failed_install_leaves_backup_and_destination_intact(self):
        self.store.backup(self.backup)
        original = self.backup.read_bytes()
        with patch('vestaforge.recovery.os.link', side_effect=OSError('disk is full')):
            result, output, errors = self.restore()
        self.assertEqual(result, 2)
        self.assertEqual(output, '')
        self.assertIn('disk is full', errors)
        self.assertFalse((self.spare / 'events.sqlite').exists())
        self.assertFalse(list(self.spare.glob('.restore-*')))
        self.assertEqual(self.backup.read_bytes(), original)

    def test_repeated_snapshot_recovery_retains_provenance(self):
        self.store.create('Pending', self.operator['id'])
        self.store.backup(self.backup)
        result, _, errors = self.restore()
        self.assertEqual(result, 0, errors)
        restored = EventStore(self.spare)
        next_backup = self.root / 'next-backup.sqlite'
        restored.backup(next_backup)
        next_spare = self.root / 'next-spare'
        result, output, errors = self.restore(source=next_backup, destination=next_spare)
        self.assertEqual(result, 0, errors)
        self.assertEqual(json.loads(output)['quarantined'], 0)
        with EventStore(next_spare).connect() as db:
            self.assertEqual(db.execute('SELECT count(*) FROM recovery_history').fetchone()[0], 2)


if __name__ == '__main__':
    unittest.main()
