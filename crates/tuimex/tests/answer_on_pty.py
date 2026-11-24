"""Answers a program's questions on a pseudo-terminal, as a person would.

    answer_on_pty.py STEPS PROGRAM [ARG...]

STEPS is a JSON list of [QUESTION, ANSWER] pairs. Each ANSWER is typed,
followed by Enter, once QUESTION has appeared in what the program wrote
after the answer before it. A QUESTION that begins with `?` may never be
asked: it is passed over when a later one is asked first. What the terminal showed goes to stdout, byte
for byte: the program's output and the terminal's echo of what was typed,
so an answer typed while echo was off is not in it. The exit status is the
program's, 124 if it had to be killed, and 125 if it ended with questions
still unanswered.
"""

import fcntl
import json
import os
import select
import signal
import struct
import sys
import termios
import time

TIMEOUT = 120

steps = json.loads(sys.argv[1])
master, slave = os.openpty()
# A terminal has a size; one with none draws nothing.
fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 80, 0, 0))
pid = os.fork()
if pid == 0:
    os.close(master)
    os.setsid()
    fcntl.ioctl(slave, termios.TIOCSCTTY, 0)
    for fd in (0, 1, 2):
        os.dup2(slave, fd)
    os.close(slave)
    os.execv(sys.argv[2], sys.argv[2:])
os.close(slave)

seen = b""
answered = 0
deadline = time.monotonic() + TIMEOUT
status = None
while status is None and time.monotonic() < deadline:
    if select.select([master], [], [], 0.05)[0]:
        try:
            chunk = os.read(master, 65536)
        except OSError:
            # Linux reports the program closing its side as an error.
            chunk = b""
        seen += chunk
        if not chunk:
            _, status = os.waitpid(pid, 0)
    else:
        done, code = os.waitpid(pid, os.WNOHANG)
        if done and not select.select([master], [], [], 0)[0]:
            status = code
    # The first question still waiting that has been asked: the next one,
    # or one after questions that may never come.
    asked = None
    for at, (question, _) in enumerate(steps):
        if question.lstrip("?").encode() in seen[answered:]:
            asked = at
            break
        if not question.startswith("?"):
            break
    if status is None and asked is not None:
        answered = len(seen)
        del steps[:asked]
        # A moment for the program to have changed the terminal's mode, as
        # it does before reading a password.
        time.sleep(0.1)
        os.write(master, steps.pop(0)[1].encode() + b"\r")

if status is None:
    os.kill(pid, signal.SIGKILL)
    os.waitpid(pid, 0)
sys.stdout.buffer.write(seen)
sys.stdout.buffer.flush()
if status is None:
    sys.exit(124)
if any(not question.startswith("?") for question, _ in steps):
    sys.exit(125)
sys.exit(os.WEXITSTATUS(status) if os.WIFEXITED(status) else 128 + os.WTERMSIG(status))
