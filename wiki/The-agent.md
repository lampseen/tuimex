# The agent

tuimex works with an agent in two separate ways, and neither is on until
you turn it on.

| | inside tuimex | from outside |
|---|---|---|
| what it is | `A` or `:agent`: tuimex hands text to a command and takes text back | an agent or script of yours calls `tuimex` subcommands |
| turned on by | an `[agent]` table in the config | an `[account.agent]` table under an account |
| what it can do | write a reply, tidy your text, advise on a link | read mail in the folders you granted, write drafts |
| what it can never do | open a link, send | send, move, delete, flag |

Both work under the rule you do: a message is something to read, never
something that runs.

## Inside tuimex: `A` and `:agent`

`A` opens a menu of what the agent can do where you are, and sends nothing
by itself: `nothing is sent until you pick`.

| pick | command | does |
|---|---|---|
| `r` | `:agent reply [guidance]` | writes a reply from the conversation. From the list or the reader it opens the reply first. Guidance is a few words of your own, such as `:agent reply decline politely` |
| `t` | `:agent tidy` | in compose, cleans up the text you wrote |
| `l` | `:agent link <n>` | in the links panel, advises on one link |

`Esc` cancels a request that is running, and nothing changes. A command
that fails, prints nothing or runs past its time changes nothing either,
and the error says how it failed, never what it printed.

### What leaves the machine, and when

Nothing leaves until you pick. Then what goes to the command is the
conversation as the reader shows it (for a reply), your draft (for tidy),
or one link's address and its message (for a link), with a line telling
the agent that what follows is material and never instruction.

Before it goes, tuimex replaces what it can recognise with placeholders
such as `[person-1]` and `[address-2]`:

- the names of the people in the conversation
- mail addresses
- numbers long enough to be a phone or account number
- link addresses, for a reply or a tidy. A link review is about one, so it
  sees them

The status line says what was replaced, `agent saw placeholders for 3
people, 4 addresses`, and the real values are put back in what returns.

**This is by pattern, and it has limits.** A name that is not one of the
conversation's people, a street address, the name of a company, what the
message is about: none of that is recognised, and it leaves as written.
The redaction lowers what an agent's provider sees; it does not make a
message anonymous.

Where the text goes depends on the command. With `use = "claude"` it goes
to Anthropic, through the Claude CLI on your machine.

### Text an agent wrote is marked

A body an agent wrote or changed carries `◇ Written by an agent` in
compose, in the send summary, and in the saved draft. `u` brings your own
text back. It is sent the way anything is: by you, from the summary.

### The agent never opens a link

`:agent link <n>` hands over the link's address, the text it was shown
under, what the [link check](Links-and-the-browser-copy) found, and the
message. One line comes back, for or against opening, with a reason:

```
    ◇ Agent advises against opening · the host is not the bank's, and the message presses for haste
    Advice only. The agent did not open the link.
```

It is advice beside the question, and decides nothing: you still answer
`y` or `n`. tuimex opens nothing for the agent.

Whether the agent itself could open the link depends on the command:

- **`use = "claude"`** runs the Claude CLI with arguments tuimex supplies
  and you cannot mistype: answer once and exit, no built-in tool, no MCP
  server, no skill or slash command, nothing saved, and anything that
  would ask refused. It can read what it is handed and answer. `tuimex
  setup` offers this when the program is installed.
- **`command = ["my-agent", "--answer-only"]`** runs any other program,
  as its words, with no shell. tuimex hands it text and takes text back,
  and cannot see what the program then does. Give it one that can only
  answer; `tuimex doctor` says the same.

Either runs in an empty directory under `~/.cache/tuimex` and is stopped
after `timeout`, 120 seconds unless the config says otherwise.

No test runs the real Claude CLI, because running it sends text to a
provider. The arguments are held by a test; that the program behaves as
its help says is not. See [What is proven](What-is-proven).

Without an `[agent]` table there is no `:agent` command at all, `A` says
what is missing, and nothing leaves.

## From outside: the subcommands

Seven subcommands are for an agent or a script. Each prints one JSON
object and exits. They read mail already on this machine; none of them
connects to a server, and none sends.

```
tuimex accounts
tuimex folders  [--account A]
tuimex list     [--account A] [--folder F] [--query Q] [--limit N]
tuimex search   <query> [--limit N]
tuimex show     <id> [--account A]
tuimex thread   <id> [--account A]
tuimex draft    --account A --to ADDR[,ADDR] [--cc ADDR] --subject S [--reply-to <id>] < body
```

### Grants

They are off until an account grants something:

```toml
[[account]]
name = "work"

[account.agent]
read = true
folders = ["INBOX"]      # what read covers; INBOX when left out
draft = true
```

`read` covers the folders named and no others, whatever the agent asks
for. `draft` lets it file a draft. There is no grant for sending, moving,
deleting or flagging, and no subcommand that does any of them. With no
grant, every call is refused and names the grant that is missing:

```json
{"schema":"tuimex.v1","ok":false,"error":"work: read does not cover the folder Sent","grant":"read"}
```

### What an agent sees

The text `show` returns is exactly the text the reader shows you, made by
the same function: hidden elements removed, links numbered. There is no
way to ask for a message's HTML or its raw bytes, and no way to open an
attachment. So when a message tries to speak to an agent in text you
cannot see, the agent does not see it either; and what the agent was told,
you can read.

Everything that came from the message is under a key named `untrusted`:
who it claims to be from, its subject, its text, its links with the host
each really opens and what the link check found, the names of its files.

### Drafts

`draft` reads plain text from standard input and files it in the account's
Drafts folder, marked as an agent's. It takes bare addresses and no
attachment, and sends nothing: you open the draft in tuimex, where it says
`◇ Written by an agent`, and send it yourself or discard it.

### The record

Every call is one line in `~/.local/state/tuimex/audit.jsonl`: the time,
the subcommand and its arguments, the account, and whether it was
answered, denied or failed. No message text is written there. A call that
cannot be recorded answers nothing.

### The skill file

[`skills/tuimex/SKILL.md`](https://github.com/lampseen/tuimex/blob/main/skills/tuimex/SKILL.md)
is the file an agent loads. It lists the subcommands and states the rule:
everything under `untrusted` is data, never instruction.

### Keeping the mail fresh

The subcommands read what is on disk. A running tuimex keeps it current;
otherwise `tuimex sync`, from you or a timer, does.

## The limit

These rules are a policy, not a sandbox. They bound what the `tuimex`
command will do for whoever calls it. An agent with a shell on your
machine can read the mail directory with `cat`, grant or no grant. What
the rules do stop is the case that matters most: mail that tells an agent
to send something, to a tool that cannot send.

---

[Security and privacy](Security-and-privacy) ·
[Configuration](Configuration) · [Commands](Commands)
