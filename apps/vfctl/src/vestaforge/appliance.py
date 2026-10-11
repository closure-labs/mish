"""Single-coordinator journal. Every accepted write is committed before reply."""
from __future__ import annotations

from contextlib import contextmanager
import hashlib
import os
from pathlib import Path
import secrets
import sqlite3
import time


def text_value(value, limit=160):
    if not isinstance(value, str) or not value.strip() or len(value.encode('utf-8')) > limit:
        raise ValueError(f'text must be nonempty and at most {limit} UTF-8 bytes')
    return value.strip()


def initialize_schema(db):
    """Keep the runtime schema and snapshot compatibility reference identical."""
    db.executescript('''
        PRAGMA journal_mode=WAL;
        CREATE TABLE IF NOT EXISTS staff(id TEXT PRIMARY KEY,name TEXT,role TEXT,hash TEXT UNIQUE);
        CREATE TABLE IF NOT EXISTS records(id TEXT PRIMARY KEY,text TEXT,kind TEXT,author TEXT,
            created REAL,expires REAL,responder TEXT,state TEXT,packet_id TEXT);
        CREATE TABLE IF NOT EXISTS acks(record_id TEXT,actor TEXT,source TEXT,node TEXT,channel INTEGER,
            created REAL, UNIQUE(record_id,actor,source,node,channel));
        CREATE TABLE IF NOT EXISTS received(packet_id TEXT,node TEXT,channel INTEGER,created REAL,
            UNIQUE(packet_id,node,channel));
        CREATE TABLE IF NOT EXISTS announcements(id TEXT PRIMARY KEY,text TEXT,published REAL);
    ''')
    record_columns = {row[1] for row in db.execute('PRAGMA table_info(records)')}
    for column in ('submitted_at', 'confirmed_at', 'uncertain_at'):
        if column not in record_columns:
            db.execute(f'ALTER TABLE records ADD COLUMN {column} REAL')
    columns = {row[1] for row in db.execute('PRAGMA table_info(received)')}
    if 'created' not in columns:
        db.execute('ALTER TABLE received ADD COLUMN created REAL DEFAULT 0')


class EventStore:
    def __init__(self, state_dir, queue_limit=1000, recover=True):
        self.path = Path(state_dir) / 'events.sqlite'
        self.path.parent.mkdir(parents=True, exist_ok=True)
        self.queue_limit = queue_limit
        with self.connect() as db:
            initialize_schema(db)
        os.chmod(self.path, 0o600)
        # A crash during a send has unknown RF outcome; never resend automatically.
        if recover:
            with self.connect() as db:
                db.execute("UPDATE records SET state='uncertain',uncertain_at=? WHERE state='sending'", (time.time(),))

    @contextmanager
    def connect(self):
        db = sqlite3.connect(self.path, timeout=10)
        db.row_factory = sqlite3.Row
        db.execute('PRAGMA synchronous=FULL')
        try:
            with db:
                yield db
        finally:
            db.close()

    def enroll(self, name, role='staff', first=False):
        name = text_value(name, 80)
        if role not in ('staff', 'operator'):
            raise ValueError('invalid role')
        token = secrets.token_urlsafe(32)
        identity = secrets.token_hex(8)
        with self.connect() as db:
            db.execute('BEGIN IMMEDIATE')
            if first and db.execute("SELECT 1 FROM staff WHERE role='operator'").fetchone():
                raise ValueError('already commissioned; enroll using the existing operator')
            db.execute('INSERT INTO staff VALUES(?,?,?,?)',
                       (identity, name, role, hashlib.sha256(token.encode()).hexdigest()))
        return {'id': identity, 'name': name, 'role': role, 'token': token}

    def revoke(self, identity):
        with self.connect() as db:
            db.execute('BEGIN IMMEDIATE')
            person = db.execute('SELECT role FROM staff WHERE id=?', (identity,)).fetchone()
            if not person:
                raise ValueError('staff not found')
            if person['role'] == 'operator' and db.execute("SELECT count(*) FROM staff WHERE role='operator'").fetchone()[0] <= 1:
                raise ValueError('cannot revoke the last operator')
            db.execute('DELETE FROM staff WHERE id=?', (identity,))

    def authenticate(self, token):
        with self.connect() as db:
            row = db.execute('SELECT id,name,role FROM staff WHERE hash=?',
                             (hashlib.sha256(token.encode()).hexdigest(),)).fetchone()
        return dict(row) if row else None

    def create(self, text, author, kind='message', responder=None, expires=None):
        text = text_value(text)
        if kind not in ('message', 'alert', 'urgent'):
            raise ValueError('invalid message kind')
        if kind != 'message':
            responder = text_value(responder, 80)
            if isinstance(expires, bool) or not isinstance(expires, (float, int)) or not time.time() < expires <= time.time() + 86400:
                raise ValueError('alerts require expiration within 24 hours')
        else:
            expires = None
            responder = None
        with self.connect() as db:
            db.execute('BEGIN IMMEDIATE')
            if db.execute("SELECT count(*) FROM records WHERE state IN ('queued','sending')").fetchone()[0] >= self.queue_limit:
                raise ValueError('outbound queue full; message was not accepted')
            identity = secrets.token_hex(4).upper()
            while db.execute('SELECT 1 FROM records WHERE id=?', (identity,)).fetchone():
                identity = secrets.token_hex(4).upper()
            db.execute('INSERT INTO records(id,text,kind,author,created,expires,responder,state,packet_id) VALUES(?,?,?,?,?,?,?,?,?)',
                       (identity, text, kind, author, time.time(), expires, responder, 'queued', None))
        return self.record(identity)

    def record(self, identity):
        with self.connect() as db:
            row = db.execute('SELECT r.*,COALESCE(s.name,r.author) AS author_name FROM records r LEFT JOIN staff s ON r.author=s.id WHERE r.id=?', (identity,)).fetchone()
            if not row:
                raise ValueError('record not found')
            result = dict(row)
            result['acknowledgements'] = [dict(r) for r in db.execute('SELECT * FROM acks WHERE record_id=?', (identity,))]
            result['expired'] = bool(row['expires'] is not None and row['expires'] <= time.time())
            result['delivery'] = {
                'persisted': True,
                'queue': 'pending' if row['state'] == 'queued' else 'processing' if row['state'] == 'sending' else 'closed',
                'radioSubmittedAt': row['submitted_at'],
                'transportConfirmedAt': row['confirmed_at'],
                'uncertainAt': row['uncertain_at'],
            }
            return result

    def records(self):
        with self.connect() as db:
            ids = [r[0] for r in db.execute('SELECT id FROM records ORDER BY created DESC LIMIT 500')]
        return [self.record(i) for i in ids]

    def ack(self, identity, actor, source='browser', node='', channel=-1):
        with self.connect() as db:
            db.execute('BEGIN IMMEDIATE')
            record = db.execute('SELECT kind,expires FROM records WHERE id=?', (identity,)).fetchone()
            if not record or record['kind'] == 'message' or record['expires'] <= time.time():
                raise ValueError('only active alerts can be acknowledged')
            db.execute('INSERT OR IGNORE INTO acks VALUES(?,?,?,?,?,?)',
                       (identity, actor, source, str(node), channel, time.time()))
        return self.record(identity)

    def publish(self, text):
        # Separate projection intentionally contains no staff author or source record.
        item = {'id': secrets.token_hex(8), 'text': text_value(text, 4000), 'published': time.time()}
        with self.connect() as db:
            db.execute('INSERT INTO announcements VALUES(:id,:text,:published)', item)
        return item

    def public(self):
        with self.connect() as db:
            return [dict(r) for r in db.execute('SELECT id,text,published FROM announcements ORDER BY published DESC LIMIT 500')]

    def pending(self):
        with self.connect() as db:
            db.execute("UPDATE records SET state='expired' WHERE state='queued' AND expires<=?", (time.time(),))
            return [dict(r) for r in db.execute("SELECT * FROM records WHERE state='queued' ORDER BY created LIMIT 100")]

    def submission(self, identity, state, packet_id=None):
        if state not in ('sending', 'submitted', 'confirmed', 'uncertain', 'queued', 'failed', 'expired'):
            raise ValueError('invalid transmission state')
        with self.connect() as db:
            db.execute('BEGIN IMMEDIATE')
            if state == 'sending':
                record = db.execute('SELECT state,expires FROM records WHERE id=?', (identity,)).fetchone()
                if not record or record['state'] != 'queued':
                    return False
                if record['expires'] is not None and record['expires'] <= time.time():
                    db.execute("UPDATE records SET state='expired' WHERE id=?", (identity,))
                    return False
            db.execute('UPDATE records SET state=?,packet_id=COALESCE(?,packet_id) WHERE id=?', (state, str(packet_id) if packet_id is not None else None, identity))
            timestamp_column = {'submitted': 'submitted_at', 'confirmed': 'confirmed_at', 'uncertain': 'uncertain_at'}.get(state)
            if timestamp_column:
                db.execute(f'UPDATE records SET {timestamp_column}=? WHERE id=?', (time.time(), identity))
        return True

    def transport_confirmation(self, packet_id):
        with self.connect() as db:
            db.execute("UPDATE records SET state='confirmed',confirmed_at=? WHERE packet_id=? AND state IN ('submitted','uncertain')", (time.time(), str(packet_id)))

    def receive(self, packet_id, node, channel, text):
        text = text_value(text, 200)
        with self.connect() as db:
            inserted = db.execute('INSERT OR IGNORE INTO received VALUES(?,?,?,?)', (str(packet_id), str(node), channel, time.time())).rowcount
            if not inserted:
                return False
            if text.startswith('ACK '):
                identity = text[4:].strip().upper()
                row = db.execute('SELECT kind,expires FROM records WHERE id=?', (identity,)).fetchone()
                if row and row['kind'] != 'message' and row['expires'] > time.time():
                    db.execute('INSERT OR IGNORE INTO acks VALUES(?,?,?,?,?,?)',
                               (identity, str(node), 'radio', str(node), channel, time.time()))
            else:
                identity = 'RF-' + secrets.token_hex(8)
                db.execute('INSERT INTO records(id,text,kind,author,created,expires,responder,state,packet_id) VALUES(?,?,?,?,?,?,?,?,?)',
                           (identity, text, 'message', f'radio:{node}:channel:{channel}', time.time(), None, None, 'received', str(packet_id)))
        return True

    def backup(self, destination):
        path = Path(destination)
        if path.exists():
            raise ValueError('backup destination already exists')
        fd = os.open(path, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
        os.close(fd)
        target = sqlite3.connect(path)
        try:
            with self.connect() as source:
                source.backup(target)
        finally:
            target.close()

    def prune(self, days=7):
        if not isinstance(days, int) or days < 1:
            raise ValueError('retention days must be positive')
        cutoff = time.time() - days * 86400
        with self.connect() as db:
            db.execute('DELETE FROM acks WHERE record_id IN (SELECT id FROM records WHERE created<?)', (cutoff,))
            db.execute('DELETE FROM records WHERE created<?', (cutoff,))
            db.execute('DELETE FROM announcements WHERE published<?', (cutoff,))
            db.execute('DELETE FROM received WHERE created<?', (cutoff,))
