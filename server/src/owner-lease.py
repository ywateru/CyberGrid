#!/usr/bin/env python3
"""POSIX OS lease for the Node owner; no coordination state is persisted.

The containing directory and lease inode are never deleted. flock releases on
helper exit; parent stdin EOF also releases after SIGKILL of Node. This avoids
stale-PID deletion and competing starters locking different inodes.
"""
import fcntl
import os
import stat
import sys
import time

try:
    directory = sys.argv[1]
    try:
        os.mkdir(directory, 0o700)
    except FileExistsError:
        pass
    dfd = os.open(directory, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
    if os.fstat(dfd).st_mode & 0o077:
        raise RuntimeError('private lease directory required')
    fd = os.open('lease', os.O_CREAT | os.O_RDWR | os.O_NOFOLLOW, 0o600, dir_fd=dfd)
    if not stat.S_ISREG(os.fstat(fd).st_mode) or os.fstat(fd).st_mode & 0o077:
        raise RuntimeError('private regular lease required')
    # Give a killed predecessor's pipe a bounded interval to reach EOF.
    deadline = time.monotonic() + 2
    while True:
        try:
            fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
            break
        except BlockingIOError:
            if time.monotonic() >= deadline:
                raise RuntimeError('another owner holds this lease')
            time.sleep(0.05)
    os.ftruncate(fd, 0)
    os.write(fd, (str(os.getppid()) + '\n').encode('ascii'))
    print('LOCKED', flush=True)
    while sys.stdin.buffer.read(1):
        pass
except Exception:
    print('Owner OS lease unavailable', file=sys.stderr)
    sys.exit(73)
