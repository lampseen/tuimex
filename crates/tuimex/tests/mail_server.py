"""Plays everyone else on the mail server `make real-server` started: another
client adding a message, or looking at what a folder holds.

    mail_server.py USER append FOLDER SUBJECT
    mail_server.py USER holds FOLDER SUBJECT COUNT
    mail_server.py USER create FOLDER
    mail_server.py USER delete FOLDER
    mail_server.py USER absent FOLDER
    mail_server.py USER put FOLDER FILE
    mail_server.py USER header FOLDER SUBJECT TEXT
    mail_server.py USER calendar FOLDER SUBJECT METHOD FROM TEXT

The server is the one TUIMEX_SERVER_HOST, _PORT, _PASSWORD and _CA name, as
`crates/tuimex-engine/tests/spike.rs` lists them; the user TUIMEX_SERVER_USER3
names signs in with TUIMEX_SERVER_PASSWORD3. `holds` waits until FOLDER is there and has
COUNT messages with SUBJECT that are not marked deleted, and fails when it
never does. `put` appends the message in FILE as it is. `header` waits until
a message with SUBJECT in FOLDER has TEXT among its headers. `absent` deletes
FOLDER if it is there, where `delete` fails when it is not. `calendar` waits
until a message with SUBJECT in FOLDER is from the address FROM and carries a
`text/calendar` part whose type says `method=METHOD` and whose text, decoded
and unfolded, holds TEXT.
"""

import email
import imaplib
import os
import ssl
import sys
import time

user, verb, folder = sys.argv[1:4]
context = ssl.create_default_context(cafile=os.environ["TUIMEX_SERVER_CA"])
server = imaplib.IMAP4_SSL(os.environ["TUIMEX_SERVER_HOST"], int(os.environ["TUIMEX_SERVER_PORT"]), ssl_context=context)
third = user == os.environ.get("TUIMEX_SERVER_USER3")
server.login(user, os.environ["TUIMEX_SERVER_PASSWORD3" if third else "TUIMEX_SERVER_PASSWORD"])


# imaplib quotes a mailbox name itself from Python 3.15; before that a
# name with a space in it reaches the server as two words.
if hasattr(imaplib.IMAP4, "_astring"):
    box = folder
else:
    box = '"' + folder.replace("\\", "\\\\").replace('"', '\\"') + '"'


def ok(answer):
    if answer[0] != "OK":
        sys.exit(f"mail_server: {verb} {folder}: {answer}")
    return answer[1]


if verb == "create":
    # A folder that is already there is the folder asked for.
    server.create(box)
elif verb == "delete":
    ok(server.delete(box))
elif verb == "absent":
    server.delete(box)
elif verb == "append":
    subject = sys.argv[4]
    stamp = time.strftime("%a, %d %b %Y %H:%M:%S +0000", time.gmtime())
    message = (
        f"From: Grace Hopper <grace@example.net>\r\nTo: {user}@example.com\r\nSubject: {subject}\r\n"
        f"Message-ID: <{subject.replace(' ', '-')}@example.net>\r\nDate: {stamp}\r\n\r\nSent from another client.\r\n"
    )
    ok(server.append(box, None, None, message.encode()))
elif verb == "holds":
    subject, count = sys.argv[4], int(sys.argv[5])
    deadline = time.monotonic() + 30
    while True:
        # A folder tuimex is about to make is not there yet: that is
        # something to wait for, like a message on its way into it.
        if server.select(box, readonly=True)[0] == "OK":
            found = len(ok(server.search(None, "UNDELETED", "SUBJECT", f'"{subject}"'))[0].split())
        else:
            found = "no folder to hold any"
        if found == count:
            break
        if time.monotonic() > deadline:
            sys.exit(f"mail_server: {folder} holds {found} of {subject!r}, not {count}")
        time.sleep(0.2)
elif verb == "put":
    with open(sys.argv[4], "rb") as file:
        ok(server.append(box, None, None, file.read()))
elif verb == "header":
    subject, text = sys.argv[4], sys.argv[5]
    deadline = time.monotonic() + 30
    while True:
        ok(server.select(box, readonly=True))
        found = ok(server.search(None, "UNDELETED", "SUBJECT", f'"{subject}"'))[0].split()
        heads = [ok(server.fetch(number, "(BODY.PEEK[HEADER])"))[0][1].decode(errors="replace") for number in found]
        # A long header is folded; unfolded, it reads as it was written.
        if any(text in head.replace("\r\n ", " ").replace("\r\n\t", " ") for head in heads):
            break
        if time.monotonic() > deadline:
            sys.exit(f"mail_server: no {subject!r} in {folder} has {text!r} in its headers: {heads}")
        time.sleep(0.2)
elif verb == "calendar":
    subject, method, sender, text = sys.argv[4:8]

    def carries(message):
        if sender not in message.get("From", ""):
            return False
        for part in message.walk():
            if part.get_content_type() != "text/calendar" or (part.get_param("method") or "").upper() != method:
                continue
            said = part.get_payload(decode=True).decode(errors="replace")
            if text in said.replace("\r\n ", "").replace("\r\n\t", ""):
                return True
        return False

    deadline = time.monotonic() + 30
    while True:
        found = []
        if server.select(box, readonly=True)[0] == "OK":
            found = ok(server.search(None, "UNDELETED", "SUBJECT", f'"{subject}"'))[0].split()
        whole = [email.message_from_bytes(ok(server.fetch(number, "(BODY.PEEK[])"))[0][1]) for number in found]
        if any(carries(message) for message in whole):
            break
        if time.monotonic() > deadline:
            kinds = [[part.get_content_type() for part in message.walk()] for message in whole]
            sys.exit(f"mail_server: no {subject!r} in {folder} from {sender} carries {method} with {text!r}: {kinds}")
        time.sleep(0.2)
else:
    sys.exit(f"mail_server: no such verb: {verb}")
server.logout()
