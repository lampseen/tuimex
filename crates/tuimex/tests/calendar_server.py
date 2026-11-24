"""Plays whoever else looks at the CalDAV server `make real-server` started:
the test that makes a calendar for a run, and reads back what tuimex wrote.

    calendar_server.py USER make URL
    calendar_server.py USER holds URL TEXT

USER signs in with TUIMEX_SERVER_PASSWORD, which is how tuimex signs in to a
calendar: as the mail account, with the mail account's password. `make` makes
a calendar at URL, where there must be none. `holds` waits until the
calendar at URL, served whole with its folded lines unfolded, holds TEXT, and
fails when it never does. URL is on this machine: nothing here leaves it.
"""

import base64
import os
import sys
import time
import urllib.error
import urllib.request

user, verb, url = sys.argv[1:4]
if not url.startswith("http://127.0.0.1:"):
    sys.exit(f"calendar_server: {url} is not on this machine")
token = base64.b64encode(f"{user}:{os.environ['TUIMEX_SERVER_PASSWORD']}".encode()).decode()
# No proxy the environment names is asked: the server is on this machine.
opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))


def ask(method):
    request = urllib.request.Request(url, method=method, headers={"Authorization": f"Basic {token}"})
    try:
        with opener.open(request, timeout=10) as answer:
            return answer.status, answer.read().decode(errors="replace")
    except urllib.error.HTTPError as error:
        return error.code, ""


if verb == "make":
    status, _ = ask("MKCALENDAR")
    if status != 201:
        sys.exit(f"calendar_server: making {url} was answered {status}")
elif verb == "holds":
    text = sys.argv[4]
    deadline = time.monotonic() + 30
    while True:
        status, body = ask("GET")
        if status == 200 and text in body.replace("\r\n ", "").replace("\r\n\t", ""):
            break
        if time.monotonic() > deadline:
            sys.exit(f"calendar_server: {url} was answered {status} and does not hold {text!r}:\n{body}")
        time.sleep(0.2)
else:
    sys.exit(f"calendar_server: no such verb: {verb}")
