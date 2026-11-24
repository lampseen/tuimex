# Attachments

A message's files are named in its header, with their sizes. In the reader,
`a` lists them:

```
  Files
▌    1  ridge-01.jpg       2.4 MB   image/jpeg   shown above
     2  ridge-02.jpg       3.1 MB   image/jpeg
     3  group-shot.heic    1.8 MB   image/heic
     4  route.gpx           84 KB   application/gpx+xml
```

`j` `k` choose, `Enter` saves, `O` opens, `Esc` closes. Nothing is saved or
opened until you ask for that one file.

## Saving

`Enter`, or `:save <n>`, writes the file to your download directory: the
system's, or `paths.downloads` in the config. The command line then says
where it is.

- The name is the message's own, made safe: a name that tries to climb out
  of the directory, such as `../../.ssh/authorized_keys`, is reduced to a
  plain file name first.
- A file is never written over. If the name is taken, the new one is saved
  beside it as `name-2.pdf`.
- The file is created readable by you alone.

`:save <n> <path>` saves under a name or into a directory of your choice.
A path that does not start with `/` or `~` is read against the download
directory. A path that names a file already there is refused, not replaced.

## Opening

`O`, or `:open <n>`, hands the file to your system's program for its type,
after a question that names the type the message claims:

```
  Open route.gpx (application/gpx+xml) in the system's program for it?
```

The program is given a private copy under `~/.cache/tuimex`, which is
removed when tuimex exits. Whatever that program then does with a
stranger's file is the program's own behaviour; the question is there so
that a file is never opened by a slip of a key.

## Pictures

A picture attached to the message and shown in its text is given room in
the reader, at most 48 cells by 12, by its shape.

Where the terminal has an image protocol (kitty, sixel or iTerm2), the
picture's own bytes are drawn there. Elsewhere the placeholder stays, with
the file's name and size in pixels, and `O` opens the picture outside.
`tuimex doctor` says which your terminal is.

Only PNG, JPEG, GIF and WebP are decoded, and never one larger than 8192
pixels on a side. Any other format is a file like any other.

This is about pictures the message carries. A picture the message keeps on
a server is never fetched by tuimex; see
[Links and the browser copy](Links-and-the-browser-copy).

Nobody has yet watched a picture being drawn: the tests check what is
handed to the terminal, and have no terminal to look at the result on.
[What is proven](What-is-proven) lists it.

## Attaching to your own message

In compose, `^a` attaches a file; `Tab` completes its path. See
[Writing and sending](Writing-and-sending).

---

[Reading mail](Reading-mail) · [Security and privacy](Security-and-privacy)
