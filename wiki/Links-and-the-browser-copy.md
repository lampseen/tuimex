# Links and the browser copy

A message is something to read. tuimex fetches nothing a message names: no
picture, no stylesheet, no font, no preview, no tracking pixel. There is no
setting that turns that on. Two things leave the terminal, and only when
you ask for that one thing: a link you open, and the sealed copy.

## Links

In the reader a link is its text and a number. `o` lists them:

```
  Links
▌    1  bank.example.com.device-check.example.net   https://bank.example.com/confirm   ▲ text says bank.example.com
     2  examplebank.example.net                     Contact us
  opens https://bank.example.com.device-check.example.net/confirm
```

The first column is the host the link really opens, whatever its text
says, and the line underneath is the whole address. A digit opens that
link; `j` `k` choose and `Enter` opens; `:link <n>` does the same by number.

### What a link is checked for

An honest link opens at once in your browser. One that shows any of these
is asked about first, naming its real host, and opens only after `y`:

| the panel says | because |
|---|---|
| `text says <host>` | the link's text names one host and the link goes to another. A subdomain of the same site does not count: `example.com` on a link to `www.example.com` deceives nobody |
| `address carries a sign-in name` | something precedes an `@` in the address, as in `https://bank.example.com@other.example.net/`, which a browser reads as going to the second host |
| `host is a bare IP address` | the host is a number, in any spelling a browser takes, and not a name |
| `host mixes alphabets` | one part of the host mixes letters of more than one script, which is how a look-alike of a known name is spelled |

In a confirm, `y` or `Enter` is yes and `n` or `Esc` is no. Any other key
cancels and is dropped, so a key typed too fast does not answer a question
it did not read.

### What never opens

Only `http`, `https` and `mailto` open. A link of any other scheme
(`file:`, `javascript:`, `data:`, an application's own) is listed with
`will not open`, and no key opens it. A `mailto` link opens compose with
its address filled in, and sends nothing.

### Tracking

Known click-tracking parameters (`utm_…`, `fbclid`, `gclid` and others of
their kind) are removed from the address before it opens, and the panel
says `tracking removed`. A link that goes through a known tracking service
says `tracking service`: opening it tells that service you clicked, and
tuimex cannot follow it for you without doing the same.

### Asking the agent about a link

With an `[agent]` table in the config, `A` in the links panel or in a
link's question, or `:agent link <n>`, asks the agent whether to open it.
The agent reads the address and the message and opens nothing; its answer
is one line of advice beside the question, and you still answer the
question. See [The agent](The-agent).

### Unsubscribing

`:unsubscribe` uses what the message's own header names, and is never
automatic. A web address is asked about by host, `Unsubscribe at <host>?`,
and opens after `y`. A mail address opens compose with the message ready
for you to read and send yourself.

## The sealed copy

`B`, or `:browser`, shows the message as its author styled it, in your own
browser, from a copy tuimex wrote. The copy is *sealed*:

- **Nothing in it runs.** Scripts, event handlers, forms, frames, redirects
  and anything that covers the page are removed.
- **Nothing in it loads.** The page begins with a content security policy
  that forbids every request, so even something the sanitiser missed has
  nowhere to go. Pictures attached to the message are placed inside the
  page itself.
- **Its layout survives.** Table sizes, alignment, and the stylesheet rules
  that only lay out and colour are kept.
- **It is private and short-lived.** It is written under
  `~/.cache/tuimex`, readable by you alone, and removed when tuimex exits,
  or at the next start if it is more than a day old.

### The question about pictures

When the message keeps pictures on a server, `B` asks each time, naming
the hosts:

```
  Load 8 images from img.fernway.example.net, cdn.example.net?  y yes · n no
```

A **yes** lets the copy load pictures from exactly those hosts, over https,
and nothing else: no script, no stylesheet, no font. Those servers then
learn that the message was opened, and from which network address, which
is what loading a picture means. A **no** opens the copy sealed, with each picture a box of
its size and a line above the message counting them. Nothing is remembered
for a sender or a message; the next `B` asks again.

**Trackers never load, whatever you answer.** A picture on a known tracking
service's host, one a pixel across, or one hidden from view is left out of
the question, the count and the copy. A message whose only remote pictures
are trackers is not asked about and opens sealed. The fixture newsletter
above has nine remote pictures; the question counts eight.

Inside tuimex nothing is fetched either way.

---

[Attachments](Attachments) · [Security and privacy](Security-and-privacy) ·
[The agent](The-agent)
