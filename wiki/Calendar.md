# Calendar

One calendar for every account: each account's own calendar, read from
its provider, the events tuimex itself holds, and the invitations that
are in your mail and in no calendar yet. `gc` opens it and `gm` returns
to mail; each side keeps where it was. `F2` and `F1` do the same by one
key each, wherever `gc` and `gm` are read: not while you are typing into
the event form, a message, the command line or a prompt.

A calendar is written by other people. Its events are text from outside,
as mail is: nothing an event names is fetched, and a link in one opens
only when you ask for that one link.

## The three views

| view | shows |
|---|---|
| **month** | the months that fit, each day marked with how much is on it, and beside them the selected day's events |
| **week** | seven days against the clock, with what lasts all day above; three days where the terminal is narrow |
| **day** | the selected day's events in order, then the days after it that have any |

`Tab` and `S-Tab` step through them, and `1` `2` `3` choose one, at the
same date. `Enter` goes deeper, from the month to the day and from a
selected event to the event's own screen: when, where, which calendar,
who was asked and what each said, its notes and links. `Esc` comes back.

Every time is shown on this machine's clock. An event written in a time
zone that neither its calendar nor this machine defines is listed with
the time as written and the zone's name, and is not placed on the clock.

## Moving and finding

| key | does |
|---|---|
| `h` `l` | previous, next day |
| `j` `k` | in the month's grid, a week down or up; elsewhere the next, previous event |
| `Alt-l` `Alt-h` | in the month, focus the list of events; back to the grid |
| `[` `]` | previous, next page: a month, a week or a day |
| `J` `K` | next, previous event, across days |
| `t` `gd` | today; go to a date, typed as `2026-12-05`, `5 dec`, `fri` or `+2w` |
| `/` then `n` `N` | find in titles and places; `Enter` lists every match a year either side of today |
| `I` | the next invitation that waits for an answer |
| `gt` `gT` | one account's events alone, and round to all |
| `C` | the calendars, each to show or hide with `Space` |

**The month's list.** The month opens with the cursor in its grid, where
it selects a day and no event. `Alt-l` moves the focus to the list beside
it, as it moves to the preview in mail: the day's first event is selected,
the grid's cursor dims, and from there every key is what it is in the day
view. `j` and `k` step through events, across days, with the grid's cursor
following; `h` and `l` still move by a day; `Enter` opens the selected
event, and `o`, `i`, `E` and `d` act on it. A day with more events than
there are rows scrolls to keep the selected one in sight. `Alt-h` or `Esc`
returns to the grid. `Alt-j` and `Alt-k` do what `Alt-l` and `Alt-h` do;
the hint names `Alt-h` because the list is beside the grid at every width.

A key that mail uses means the same here, with one exception: `h` and
`l` move by a day here and by a section in mail. A command that acts on
rows of mail is refused while the calendar is open, and says `gm` returns
to mail.

## Answering an invitation

`i` on an event you were asked to, or on an invitation's card in the
reader, asks `Answer?`:

| key | does |
|---|---|
| `a` `t` `d` | accept, tentative, decline |
| `p` | propose another time, typed as a date is: `thu 14:00` |
| `x` | ignore: nothing is sent, and the event is no longer drawn |

`:invite accept` and the rest do the same.

**Where the answer goes.** A reply is mailed to the organizer, and to
nobody else, from the account the invitation came to. It waits in the
Outbox for the send delay like any mail. The event, with your answer, is
written to the calendar of that same account, and the result line says
which: `in work` when tuimex writes that account's calendar at its
provider, `in work, local` when it is kept on this machine (see
[What `local` means](#what-local-means)).

**`u` takes it back**: the calendar entry, and the reply out of the
Outbox, as long as the reply has not left. `u` on the reply's own row in
the Outbox does the same. Once the reply has been sent the answer stands;
answer again to change it.

**Proposing a time** mails the organizer the new start, as a calendar
message their program may understand and as a sentence a person can read
either way. The event is shown as proposed until the organizer sends a
new version.

**An invitation in mail is on the calendar.** One that is in an Inbox
and in no calendar is drawn on its day with `needs answer`, and is stored
nowhere until you answer it. An account whose provider adds invitations
to the calendar by itself shows the provider's copy instead, once.

**What the organizer sends later** is applied to an event tuimex holds:
a new version replaces it and asks again, and a cancellation removes it.
Each is said once, on the command line and in `:messages`. Both are
believed only from the organizer the event already names: mail is
hostile, and anyone can send a cancellation. An event at a provider is
the provider's to update. A guest's answer to a local event you organize
is kept in the event and said the same way.

## Joining a meeting

`o` lists the selected event's links, the meeting's first, each with the
host it really opens. `Enter` or its number opens one in your browser.
A link is [checked as a link in mail is](Links-and-the-browser-copy):
one that is neither web nor mail is refused, one whose text names
another host asks first, and `A` asks the agent about it where one is
configured. The agent opens nothing.

## Making and changing events

`c`, or `:event [title]`, opens the form for a new event on the selected
day; `E` opens it on the selected event.

| key | does |
|---|---|
| `Tab` `S-Tab` | next, previous field |
| `h` `l` | choose, on Calendar, All day, Repeats and Reminder; `Left` `Right` do the same. In the other fields they are letters |
| `Enter` | write the notes in your editor |
| `^s` | save |
| `Esc` | close; with changes, it asks whether to keep them |

Dates are typed as words, and the status line says what they were taken
to mean before anything is saved. **Calendar** offers each account's
local calendar, and the account's own where tuimex writes it. **Guests**
are addresses, by comma or by line. An event with guests asks before it
is saved, naming how many will be mailed the invitation; only `y` saves
and sends.

`d` deletes the selected event, and `u` brings it back. An event that
repeats asks first, for `d` and for `E`: `t` is this one day, `a` all of
them. Deleting an event you organize mails its guests that it is
cancelled; that mail waits in the Outbox too, and `u` stops it.

## Reminders

While tuimex is open, an event's reminder rings the terminal's bell, puts
the event and how soon it begins on the status line, and raises a desktop
notification with the event's title and nothing else. It falls due at the
event's own alarms, or ten minutes ahead where it has none; an all-day
event with no alarm has none. A declined event, an ignored invitation and
an event of a calendar you have hidden remind nobody.

`gn` and `:notify on|off` switch reminders, and the choice is remembered.
`[notify]` in the [config](Configuration) sets the lead time and how the
notification is raised. Nothing runs once tuimex is closed: a closed
terminal reminds nobody.

## What `local` means

An event marked `local` is a file on this machine, under the data
directory, and nowhere else: not on your phone, and not in the provider's
own apps. tuimex keeps an event locally when there is no provider's
calendar it may write: an account with no `[[calendar]]`, one read from a
feed, and a CalDAV calendar whose `write` is not `true`.

An answer kept locally and the provider's own copy of the same event are
drawn as one event: the provider's copy is the event, and your answer is
shown on it until the provider's copy has an answer of its own.

`:export [path]` writes every local event to one `.ics` file, in the
download directory unless you name a path, for importing into a
provider's calendar by hand. It never writes over a file.

## What is read-only, and why

| calendar | read | written |
|---|---|---|
| a **feed** (`kind = "feed"`) | yes | never: a feed's address only serves the calendar |
| **CalDAV** with no `write` | yes | no: answers and new events are kept locally |
| **CalDAV** with `write = true` | yes | yes |

`d` and `E` on an event of a calendar that is only read are refused, and
say to change it where the calendar is kept. tuimex never changes a
provider's event it could not then write back.

`write` is false until a write has been tried, because nobody has ever
written to Google's calendar this way and tuimex does not write to a
calendar on an untried path. `tuimex setup` tries it when you say so; see
below.

## Setting up Google and Proton

**Google, with an app password.** `tuimex setup` offers to read the
calendar over CalDAV with the app password the account already signs in
with. It then offers to find out whether the calendar can be written: it
adds one event named "tuimex write test", a year from today, removes it
again, and sets `write = true` when both worked. Say no and the calendar
is read, and what you do for it is kept locally. For a calendar already
in the config, choose *Find out whether a calendar that is only read can
be written* from setup's menu.

**Google, signed in through the browser,** and **Proton** are read from
the calendar's private address, a feed, which setup asks for unseen and
keeps in the keyring. A feed is read-only. For Proton that is all there
is: Proton Bridge carries mail, not calendars, so an invitation answered
here is `local`, and Proton's own apps do not see the answer although the
organizer has it.

[Setup and accounts](Setup-and-accounts#calendars) has the steps, and
[Configuration](Configuration#calendar-at-most-once-for-each-account) the
keys.

## What has not been tried

Every test of the calendar runs against stand-ins or a CalDAV server in a
container. Google's and Proton's own servers, writing to Google's
calendar, the desktop's notification programs and a Mac have never had
this run against them: [What is proven](What-is-proven#calendars) says
which.

One thing seen on the test server: it refuses a wrong password and hangs
up in a way that sometimes loses its answer, and tuimex then says "the
connection failed" where it would have said the sign-in was refused. If
a calendar says that just after its password changed, suspect the
password. Whether Google's server does this is not known.

---

[Reading mail](Reading-mail) · [Keys](Keys) · [Commands](Commands) ·
[Setup and accounts](Setup-and-accounts)
