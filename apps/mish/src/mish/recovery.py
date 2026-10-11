"""Offline recovery into a fresh event database; never replay snapshot work."""
from __future__ import annotations

import hashlib
import os
from pathlib import Path
import sqlite3
import tempfile
import time

from .appliance import initialize_schema


RECOVERY_SCHEMA = 'CREATE TABLE IF NOT EXISTS recovery_history(restored REAL,source_sha256 TEXT,quarantined INTEGER)'


def _unique_indexes(db, table):
    return {
        (tuple(row[0] for row in db.execute('SELECT name FROM pragma_index_info(?) ORDER BY seqno', (index[1],))), index[4])
        for index in db.execute('SELECT * FROM pragma_index_list(?)', (table,)) if index[2]
    }


def _validate(db):
    db.execute('PRAGMA trusted_schema=OFF')
    if db.execute('PRAGMA integrity_check').fetchall() != [('ok',)]:
        raise ValueError('backup failed SQLite integrity validation')
    if db.execute("SELECT 1 FROM sqlite_schema WHERE type IN ('trigger','view')").fetchone():
        raise ValueError('backup contains unsupported database objects')
    tables = {row[0] for row in db.execute("SELECT name FROM sqlite_schema WHERE type='table'")}
    reference = sqlite3.connect(':memory:')
    try:
        initialize_schema(reference)
        if 'recovery_history' in tables:
            reference.execute(RECOVERY_SCHEMA)
        for table, in reference.execute("SELECT name FROM sqlite_schema WHERE type='table'"):
            expected = reference.execute('SELECT * FROM pragma_table_xinfo(?)', (table,)).fetchall()
            columns = db.execute('SELECT * FROM pragma_table_xinfo(?)', (table,)).fetchall()
            # Order, types, defaults, primary keys and hidden/generated columns
            # matter because runtime writes include positional INSERTs.
            if columns != expected or _unique_indexes(db, table) != _unique_indexes(reference, table):
                raise ValueError(f'backup has an incompatible {table} schema')
    finally:
        reference.close()


def restore_snapshot(source, state_dir):
    """Validate/stage a standalone backup and install without replacing any state.

    Stop the service and preserve its old state directory before invoking this.
    Snapshot queue entries have an unknowable later RF outcome, so quarantine
    them. This is intentionally different from restarting the same database.
    """
    source = Path(source).resolve()
    state = Path(state_dir).resolve()
    destination = state / 'events.sqlite'
    temporary = None
    try:
        if not source.is_file():
            raise ValueError('restore input must be an existing standalone backup file')
        if any(Path(str(source) + suffix).exists() for suffix in ('-wal', '-shm', '-journal')):
            raise ValueError('restore requires a standalone backup, not a live SQLite database')
        if any(os.path.lexists(str(destination) + suffix) for suffix in ('', '-wal', '-shm', '-journal')):
            raise ValueError('refusing to overwrite existing event state; preserve it while the service is stopped')
        state.mkdir(parents=True, exist_ok=True, mode=0o700)
        descriptor, name = tempfile.mkstemp(prefix='.restore-', suffix='.sqlite', dir=state)
        temporary = Path(name)
        digest = hashlib.sha256()
        with os.fdopen(descriptor, 'wb') as staged, source.open('rb') as original:
            before = os.fstat(original.fileno())
            while chunk := original.read(1024 * 1024):
                staged.write(chunk)
                digest.update(chunk)
            after = os.fstat(original.fileno())
            if (before.st_size, before.st_mtime_ns) != (after.st_size, after.st_mtime_ns):
                raise ValueError('backup changed during restore')
        if any(Path(str(source) + suffix).exists() for suffix in ('-wal', '-shm', '-journal')):
            raise ValueError('restore requires a standalone backup, not a live SQLite database')
        db = sqlite3.connect(temporary)
        try:
            _validate(db)
            # The installed snapshot is a single durable file with no sidecars.
            db.execute('PRAGMA journal_mode=DELETE')
            db.execute('PRAGMA synchronous=FULL')
            restored_at = time.time()
            with db:
                quarantined = db.execute(
                    "UPDATE records SET state='uncertain',uncertain_at=? WHERE state IN ('queued','sending')",
                    (restored_at,)).rowcount
                db.execute(RECOVERY_SCHEMA)
                db.execute('INSERT INTO recovery_history VALUES(?,?,?)',
                           (restored_at, digest.hexdigest(), quarantined))
        finally:
            db.close()
        with temporary.open('rb') as staged:
            os.fsync(staged.fileno())
        # Hard linking within the directory publishes the complete file atomically
        # and fails if another process has created the destination in the meantime.
        if any(os.path.lexists(str(destination) + suffix) for suffix in ('-wal', '-shm', '-journal')):
            raise ValueError('refusing to overwrite existing event state')
        os.link(temporary, destination)
        temporary.unlink()
        directory_fd = os.open(state, os.O_RDONLY | os.O_DIRECTORY)
        try:
            os.fsync(directory_fd)
        finally:
            os.close(directory_fd)
        return {'restored': str(destination), 'sourceSha256': digest.hexdigest(),
                'quarantined': quarantined, 'restoredAt': restored_at}
    except (OSError, sqlite3.Error) as exc:
        raise ValueError(f'cannot restore snapshot: {exc}') from exc
    finally:
        if temporary is not None:
            for suffix in ('', '-wal', '-shm', '-journal'):
                Path(str(temporary) + suffix).unlink(missing_ok=True)
