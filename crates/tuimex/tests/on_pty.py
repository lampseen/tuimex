"""Runs a program on a pseudo-terminal, the way a terminal emulator would.

    on_pty.py KEYS PROGRAM [ARG...]

KEYS are typed once the program has switched to the alternate screen. What
the program wrote to the terminal goes to stdout, byte for byte, and the exit
status is the program's (124 if it had to be killed).

With ON_PTY_STEPS=SEPARATOR in the environment KEYS is split on SEPARATOR and
each piece is typed once a frame has followed the piece before it and nothing
more has been drawn for a moment. A piece that is one key is therefore read,
and has its frame, before the next arrives: a lone Esc is Esc and not the
start of what follows it, and a frame that was on its way for another reason,
such as a countdown, is not taken for the answer to a key. Keys that are
waiting together share one frame, so a test of each key's frame types them
this way. ON_PTY_PAUSE=SECONDS also waits that long between pieces.

With ON_PTY_RESIZE=ROWSxCOLS in the environment the terminal starts at 24x80
as always, becomes that size once the first frame is complete, and KEYS are
typed once a frame has been drawn at the new size.

With ON_PTY_SCRIPT=FILE in the environment KEYS is ignored and the file's
lines are carried out in order, against the screen as a terminal would show
it (ON_PTY_SIZE=ROWSxCOLS sets its size). Nothing is typed by a line that
does not say `keys`, so what a `wait` sees arrived by itself.

    keys TEXT    type TEXT; \\r, \\x1b and the like are read as Python reads them
    wait TEXT    until some row of the screen holds TEXT
    gone TEXT    until no row of the screen holds TEXT
    run COMMAND  run COMMAND with a shell, which must succeed
    until COMMAND  run COMMAND with a shell, again and again, until it succeeds

A line that is not done within STEP_TIMEOUT seconds ends the run with status
3, and the screen as it was goes to stderr.
"""

import fcntl
import os
import select
import signal
import struct
import sys
import termios
import time

import codecs
import re
import subprocess

ENTERED = b"\x1b[?1049h"
# The end of a synchronised update, which is the end of a frame.
FRAME = b"\x1b[?2026l"
TIMEOUT = 20
# How long the terminal is quiet before the next piece is typed.
QUIET = 0.05

separator = os.environ.pop("ON_PTY_STEPS", None)
steps = [step.encode() for step in (sys.argv[1].split(separator) if separator else [sys.argv[1]]) if step]
pause = float(os.environ.pop("ON_PTY_PAUSE", "0"))
resize = os.environ.pop("ON_PTY_RESIZE", None)
frames_before_keys = 0
not_before = 0.0
last_drawn = 0.0
master, slave = os.openpty()
# Sized before the program starts, so its first frame is 80x24.
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

STEP_TIMEOUT = int(os.environ.get("ON_PTY_STEP_TIMEOUT", "40"))
# A control sequence, an operating system command, or any other escape.
ESCAPE = re.compile(r"\x1b(?:\[([0-9;?]*)([@-~])|\][^\x07\x1b]*(?:\x07|\x1b\\)|[^\[\]])")


class Screen:
    """As much of a terminal as tuimex writes to: where the cursor is put,
    what is erased, and the characters."""

    def __init__(self, rows, cols):
        self.rows, self.cols = rows, cols
        self.grid = [[" "] * cols for _ in range(rows)]
        self.row = self.col = 0
        self.decode = codecs.getincrementaldecoder("utf-8")("replace")
        self.pending = ""

    def feed(self, data):
        text = self.pending + self.decode.decode(data)
        # An escape cut in two by a read is finished by the next one.
        cut = text.rfind("\x1b")
        if cut >= 0 and not ESCAPE.match(text, cut):
            text, self.pending = text[:cut], text[cut:]
        else:
            self.pending = ""
        at = 0
        while at < len(text):
            escape = ESCAPE.match(text, at)
            if escape:
                self.control(escape.group(1) or "", escape.group(2))
                at = escape.end()
                continue
            char = text[at]
            at += 1
            if char == "\r":
                self.col = 0
            elif char == "\n":
                self.row = min(self.row + 1, self.rows - 1)
            elif char >= " " and self.row < self.rows and self.col < self.cols:
                self.grid[self.row][self.col] = char
                self.col += 1

    def control(self, params, final):
        numbers = [int(n) if n.isdigit() else 0 for n in params.split(";")]
        if final in "Hf":
            self.row = max(numbers[0], 1) - 1
            self.col = max((numbers + [1])[1], 1) - 1
        elif final == "J":
            self.grid = [[" "] * self.cols for _ in range(self.rows)]
        elif final == "K" and self.row < self.rows:
            self.grid[self.row][self.col:] = [" "] * (self.cols - self.col)

    def lines(self):
        return ["".join(row).rstrip() for row in self.grid]

    def holds(self, text):
        return any(text in line for line in self.lines())


def script(path):
    """Carries out the lines of `path`; the program's exit status, or None
    when it was still running after the last of them."""
    rows, cols = (int(n) for n in os.environ.pop("ON_PTY_SIZE", "24x80").split("x"))
    fcntl.ioctl(master, termios.TIOCSWINSZ, struct.pack("HHHH", rows, cols, 0, 0))
    os.kill(pid, signal.SIGWINCH)
    screen, output, status = Screen(rows, cols), b"", None

    def read(wait):
        nonlocal output, status
        if status is None and select.select([master], [], [], wait)[0]:
            try:
                chunk = os.read(master, 65536)
            except OSError:
                chunk = b""
            if not chunk:
                status = os.waitpid(pid, 0)[1]
            output += chunk
            screen.feed(chunk)

    with open(path, encoding="utf-8") as lines:
        steps = [line.rstrip("\n").split(" ", 1) for line in lines if line.strip()]
    for verb, rest in steps:
        deadline = time.monotonic() + STEP_TIMEOUT
        if verb == "keys":
            os.write(master, rest.encode().decode("unicode_escape").encode())
        elif verb == "run":
            subprocess.run(rest, shell=True, check=True)
        quiet = {"stdout": subprocess.DEVNULL, "stderr": subprocess.DEVNULL}

        def so():
            if verb == "until":
                return subprocess.run(rest, shell=True, **quiet).returncode == 0
            return verb not in ("wait", "gone") or screen.holds(rest) == (verb == "wait")

        while not so():
            read(0.05)
            if status is not None or time.monotonic() > deadline:
                sys.stderr.write(f"on_pty: `{verb} {rest}` was not so; the screen:\n" + "\n".join(screen.lines()) + "\n")
                if status is None:
                    os.kill(pid, signal.SIGKILL)
                    os.waitpid(pid, 0)
                sys.stdout.buffer.write(output)
                sys.exit(3)
    deadline = time.monotonic() + TIMEOUT
    while status is None and time.monotonic() < deadline:
        read(0.05)
    return output, status


if "ON_PTY_SCRIPT" in os.environ:
    seen, status = script(os.environ.pop("ON_PTY_SCRIPT"))
    if status is None:
        os.kill(pid, signal.SIGKILL)
        os.waitpid(pid, 0)
    sys.stdout.buffer.write(seen)
    sys.stdout.buffer.flush()
    if status is None:
        sys.exit(124)
    sys.exit(os.WEXITSTATUS(status) if os.WIFEXITED(status) else 128 + os.WTERMSIG(status))

seen = b""
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
        last_drawn = time.monotonic()
        if resize and FRAME in seen:
            rows, cols = (int(n) for n in resize.split("x"))
            # The kernel tells the program, as it does when a window changes.
            fcntl.ioctl(master, termios.TIOCSWINSZ, struct.pack("HHHH", rows, cols, 0, 0))
            resize = None
            frames_before_keys = seen.count(FRAME) + 1
        if not chunk:
            _, status = os.waitpid(pid, 0)
    else:
        done, code = os.waitpid(pid, os.WNOHANG)
        if done and not select.select([master], [], [], 0)[0]:
            status = code
    typing = steps and not resize and ENTERED in seen and seen.count(FRAME) >= frames_before_keys
    quiet = separator is None or time.monotonic() - last_drawn >= QUIET
    if status is None and typing and quiet and time.monotonic() >= not_before:
        os.write(master, steps.pop(0))
        frames_before_keys = seen.count(FRAME) + 1
        not_before = time.monotonic() + pause

if status is None:
    os.kill(pid, signal.SIGKILL)
    os.waitpid(pid, 0)
sys.stdout.buffer.write(seen)
sys.stdout.buffer.flush()
if status is None:
    sys.exit(124)
sys.exit(os.WEXITSTATUS(status) if os.WIFEXITED(status) else 128 + os.WTERMSIG(status))
