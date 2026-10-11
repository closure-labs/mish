from __future__ import annotations

import tempfile
import time
import unittest
from pathlib import Path
from aiohttp.test_utils import TestClient, TestServer
from mish.appliance import EventStore
from mish.server import create_app
from mish.storage import MishStore, default_config


class JournalTests(unittest.TestCase):
    def test_queue_restart_ack_backup_and_exhaustion(self):
        with tempfile.TemporaryDirectory() as directory:
            store = EventStore(directory, queue_limit=1)
            person = store.enroll('operator', 'operator', first=True)
            item = store.create('urgent private', person['id'], 'urgent', 'Gate lead', time.time()+60)
            with self.assertRaisesRegex(ValueError, 'queue full'):
                store.create('overflow', person['id'])
            store.ack(item['id'], person['id'])
            store.backup(Path(directory)/'backup.sqlite')
            restored_dir = Path(directory)/'restored'
            restored_dir.mkdir()
            import shutil
            shutil.copyfile(Path(directory)/'backup.sqlite', restored_dir/'events.sqlite')
            restored = EventStore(restored_dir)
            self.assertEqual(restored.record(item['id'])['acknowledgements'][0]['actor'], person['id'])
            self.assertIsNotNone(restored.authenticate(person['token']))
            restarted = EventStore(directory)
            self.assertEqual(restarted.pending()[0]['id'], item['id'])
            self.assertEqual(restarted.record(item['id'])['acknowledgements'][0]['actor'], person['id'])
            self.assertIsNotNone(restarted.authenticate(person['token']))
            self.assertEqual(restarted.public(), [])
            self.assertEqual((Path(directory)/'backup.sqlite').stat().st_mode & 0o777, 0o600)
            with self.assertRaises(ValueError):
                restarted.enroll('second operator', 'operator', first=True)

    def test_traversal_and_malformed_identifiers(self):
        with tempfile.TemporaryDirectory() as directory:
            store = MishStore(directory)
            for identity in ('../escape', '/tmp/escape', '..', 'a/b', 'a\\b', ['bad'], '', 'a'*129):
                with self.subTest(identity=identity), self.assertRaises(ValueError):
                    store.waypoint_path(identity)
                with self.assertRaises(ValueError):
                    store.geopack_path(identity)
            with self.assertRaises(ValueError):
                store.add_simulated_waypoint(None, 1)
            with self.assertRaises(ValueError):
                store.create_geopack('bad', 'not-a-list')

    def test_local_handle_does_not_recover_live_send_and_revoke(self):
        with tempfile.TemporaryDirectory() as directory:
            store = EventStore(directory)
            owner = store.enroll('owner', 'operator', first=True)
            staff = store.enroll('staff')
            item = store.create('sending', owner['id'])
            store.submission(item['id'], 'sending')
            local = EventStore(directory, recover=False)
            self.assertEqual(local.record(item['id'])['state'], 'sending')
            store.submission(item['id'], 'submitted', 'packet-1')
            store.transport_confirmation('packet-1')
            delivered = local.record(item['id'])['delivery']
            self.assertIsNotNone(delivered['radioSubmittedAt'])
            self.assertIsNotNone(delivered['transportConfirmedAt'])
            self.assertTrue(delivered['persisted'])
            self.assertEqual(local.record(item['id'])['acknowledgements'], [])
            local.revoke(staff['id'])
            self.assertIsNone(store.authenticate(staff['token']))
            with self.assertRaises(ValueError):
                local.revoke(owner['id'])

    def test_atomic_outbox_claim_rejects_duplicate_and_expired_work(self):
        with tempfile.TemporaryDirectory() as directory:
            store = EventStore(directory)
            item = store.create('outgoing', 'staff')
            other = EventStore(directory, recover=False)
            self.assertTrue(store.submission(item['id'], 'sending'))
            self.assertFalse(other.submission(item['id'], 'sending'))
            alert = store.create('expires', 'staff', 'alert', 'lead', time.time()+60)
            with store.connect() as db:
                db.execute('UPDATE records SET expires=0 WHERE id=?', (alert['id'],))
            self.assertFalse(other.submission(alert['id'], 'sending'))
            self.assertEqual(store.record(alert['id'])['state'], 'expired')

    def test_retention_cleans_private_public_and_acks(self):
        with tempfile.TemporaryDirectory() as directory:
            store = EventStore(directory)
            item = store.create('old', 'staff', 'alert', 'lead', time.time()+30)
            store.ack(item['id'], 'staff')
            store.publish('old public')
            with store.connect() as db:
                db.execute('UPDATE records SET created=0')
                db.execute('UPDATE announcements SET published=0')
            store.prune(7)
            self.assertEqual(store.records(), [])
            self.assertEqual(store.public(), [])
            with store.connect() as db:
                self.assertEqual(db.execute('SELECT COUNT(*) FROM acks').fetchone()[0], 0)


class APITests(unittest.IsolatedAsyncioTestCase):
    async def asyncSetUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.config = default_config(self.temp.name)
        self.config['radio']['adapter'] = 'disabled'
        self.config['radio']['simulator']['enable'] = True
        self.store = MishStore(self.temp.name, self.config)
        self.events = EventStore(self.temp.name)
        self.owner = self.events.enroll('owner', 'operator', first=True)
        self.staff = self.events.enroll('staff')
        self.client = TestClient(TestServer(create_app(self.store, self.config)))
        await self.client.start_server()

    async def asyncTearDown(self):
        await self.client.close()
        self.temp.cleanup()

    def headers(self, person):
        return {'Authorization': 'Bearer '+person['token']}

    async def test_auth_public_projection_and_operator_permissions(self):
        for path in ('/api/messages', '/waypoints', '/geopacks'):
            self.assertEqual((await self.client.get(path)).status, 401)
        self.assertEqual((await self.client.post('/api/messages', json={'text':'secret'})).status, 401)
        response = await self.client.post('/api/messages', json={'text':'private staff position'}, headers=self.headers(self.staff))
        self.assertEqual(response.status, 201)
        self.assertEqual((await self.client.post('/api/announcements', json={'text':'leak'}, headers=self.headers(self.staff))).status, 403)
        self.assertEqual((await self.client.post('/api/staff', json={'name':'rogue'}, headers=self.headers(self.staff))).status, 403)
        public = await (await self.client.get('/public/announcements')).json()
        self.assertEqual(public, {'announcements': []})
        await self.client.post('/api/announcements', json={'text':'Gates open'}, headers=self.headers(self.owner))
        public = await (await self.client.get('/public/announcements')).json()
        self.assertEqual(set(public['announcements'][0]), {'id','text','published'})
        self.assertNotIn('private staff position', str(public))
        self.assertNotIn(self.staff['id'], str(public))
        self.assertEqual((await self.client.post('/public/announcements', json={'text':'attendee'})).status, 405)

    async def test_two_sessions_alert_ack_restart(self):
        for person in (self.owner, self.staff):
            result = await self.client.post('/api/messages', json={'text':person['name']+' message'}, headers=self.headers(person))
            self.assertEqual(result.status, 201)
        alert = await (await self.client.post('/api/alerts', json={'text':'Find lead','responder':'Pat','expires':time.time()+60},headers=self.headers(self.owner))).json()
        ack = await self.client.post('/api/alerts/'+alert['id']+'/ack', json={}, headers=self.headers(self.staff))
        self.assertEqual(ack.status, 200)
        await self.client.close()
        self.client = TestClient(TestServer(create_app(self.store, self.config)))
        await self.client.start_server()
        data = await (await self.client.get('/api/messages',headers=self.headers(self.staff))).json()
        self.assertEqual(len(data['messages']),3)
        record = next(m for m in data['messages'] if m['id']==alert['id'])
        self.assertEqual(record['acknowledgements'][0]['actor'],self.staff['id'])
        self.assertEqual(await (await self.client.get('/public/announcements')).json(), {'announcements':[]})

    async def test_bad_and_oversized_requests(self):
        headers=self.headers(self.owner)
        for value in ([], {'text':[]}, {'text':'x'*161}, {'text':None}):
            response=await self.client.post('/api/messages',json=value,headers=headers)
            self.assertEqual(response.status,400)
        response=await self.client.post('/api/messages',data='{broken',headers={**headers,'Content-Type':'application/json'})
        self.assertEqual(response.status,400)
        response=await self.client.post('/api/messages',json={'text':'x'*17000},headers=headers)
        self.assertEqual(response.status,413)
        response=await self.client.post('/waypoints/simulated',json={'id':'../escape','lat':1,'lon':1},headers=headers)
        self.assertEqual(response.status,400)
        self.assertFalse((Path(self.temp.name)/'escape.json').exists())
        for position in ([], None, 'bad'):
            response=await self.client.post('/waypoints/simulated', json={'position':position}, headers=headers)
            self.assertEqual(response.status,400)
        for expires in (None, True, 'soon', time.time()-1, float('inf')):
            response=await self.client.post('/api/alerts',json={'text':'bad','responder':'Pat','expires':expires},headers=headers)
            self.assertEqual(response.status,400)


if __name__ == '__main__':
    unittest.main()
