use tuimex_core::{AccountId, Address, Identity, Listed, Origin};
use tuimex_fixtures::design as fx;

use super::quote::{Opening, written};
use super::summary::{self, Signing, Summary};
use super::{Compose, Field, conversation, names, thread};
use crate::list;
use crate::span::Line;
use crate::when::stamp;

fn opening(identities: &[Identity]) -> Opening<'_> {
    Opening { identities, key: "draft-new".into() }
}

fn mail(key: &str) -> Listed {
    fx::inbox().into_iter().find(|m| m.envelope.id.key == key).unwrap()
}

fn text(lines: &[Line]) -> Vec<String> {
    lines.iter().map(|line| line.iter().map(|s| s.text.as_str()).collect::<String>().trim_end().to_string()).collect()
}

fn person(name: &str, email: &str) -> Address {
    Address { name: Some(name.to_string()), email: email.to_string() }
}

/// As mutt quotes: an attribution line, then `> ` before every line.
#[test]
fn a_reply_quotes_the_message_under_who_wrote_it_and_when() {
    let ids = fx::identities();
    let reply = opening(&ids).reply(&mail("halden-3"), false);
    let want = "On 23 Nov 11:32, Priya Raman wrote:\n> Hi Robin,\n>\n> One more change before you sign: our bank details \
                have changed, and the new ones are in the attached PDF. Please use them for the first invoice and \
                confirm today, because the board date has moved up.\n>\n> Priya\n";
    assert_eq!(reply.body, want);
    assert_eq!(reply.focus, Field::Body, "ready for Enter and the editor");
    assert!(!reply.dirty, "a reply nobody has touched closes without a question");
    let quoted_again = Listed { text: Some("> earlier\nnow".into()), ..mail("halden-3") };
    assert!(opening(&ids).reply(&quoted_again, false).body.ends_with("wrote:\n> > earlier\n> now\n"));
    let unread = opening(&ids).reply(&mail("halden-2"), false);
    assert_eq!(
        unread.body,
        "On 23 Nov 10:02, Dmitri Volkov wrote:\n> Fine with being a named contact. 45 days works for me.\n"
    );
}

#[test]
fn a_reply_carries_the_headers_that_keep_it_in_its_thread() {
    let ids = fx::identities();
    let reply = opening(&ids).reply(&mail("halden-3"), false);
    assert_eq!(reply.subject, "Re: Contract draft v3 for the Halden account", "one Re:, not two");
    assert_eq!(reply.in_reply_to.as_deref(), Some("<halden-3@design.example.net>"));
    let chain = ["<halden-1@design.example.net>", "<halden-2@design.example.net>", "<halden-3@design.example.net>"];
    assert_eq!(reply.references, chain);
    let first = opening(&ids).reply(&mail("halden-1"), false);
    assert_eq!(first.subject, "Re: Contract draft v3 for the Halden account");
    assert_eq!((first.in_reply_to.as_deref(), first.references.len()), (Some("<halden-1@design.example.net>"), 1));
    let shouting = Listed {
        envelope: tuimex_core::Envelope { subject: "RE: loud".into(), ..mail("rack").envelope },
        ..mail("rack")
    };
    assert_eq!(opening(&ids).reply(&shouting, false).subject, "RE: loud");
}

#[test]
fn a_reply_goes_to_the_sender_and_reply_all_copies_everyone_else_but_the_user() {
    let ids = fx::identities();
    let one = opening(&ids).reply(&mail("cabin-5"), false);
    assert_eq!((one.to.as_str(), one.cc.as_str()), ("Jonas Becker <jonas@example.com>", ""));
    let all = opening(&ids).reply(&mail("cabin-5"), true);
    assert_eq!(all.to, "Jonas Becker <jonas@example.com>");
    assert_eq!(
        all.cc,
        "Tomas Ferreira <tomas@example.net>, Greta Holm <greta@example.com>, Wen Zhao <wen@example.com>"
    );
    assert_eq!(ids[all.identity].address.email, "robin@example.com", "from the identity the message was sent to");
    let work = opening(&ids).reply(&mail("halden-2"), true);
    assert_eq!(
        (work.to.as_str(), work.cc.as_str()),
        ("Dmitri Volkov <dmitri@example.org>", "Priya Raman <priya@example.org>")
    );
    assert_eq!(ids[work.identity].address.email, "robin.vale@example.org");
    // A reply to one's own message goes to whom that message went.
    let own = opening(&ids).reply(&fx::outbox()[0], true);
    assert_eq!((own.to.as_str(), own.cc.as_str()), ("Tomas Ferreira <tomas@example.net>", ""));
}

/// As mutt forwards inline: the message between two marker lines.
#[test]
fn a_forward_is_the_message_between_two_markers_and_answers_nothing() {
    let ids = fx::identities();
    let forward = opening(&ids).forward(&mail("halden-3"));
    assert_eq!(forward.subject, "[priya@example.net: Re: Contract draft v3 for the Halden account]");
    let body: Vec<&str> = forward.body.lines().collect();
    let head = [
        "",
        "----- Forwarded message from Priya Raman <priya@example.net> -----",
        "",
        "Date: 23 Nov 11:32",
        "From: Priya Raman <priya@example.net>",
        "To: Robin Vale <robin.vale@example.org>",
        "Subject: Re: Contract draft v3 for the Halden account",
        "",
        "Hi Robin,",
    ];
    assert_eq!(body[..9], head);
    assert_eq!(body[body.len() - 3..], ["Priya", "", "----- End forwarded message -----"]);
    assert!(
        opening(&ids).forward(&mail("halden-1")).body.contains("\nCc: Dmitri Volkov <dmitri@example.org>\nSubject: ")
    );
    assert_eq!((forward.to.as_str(), forward.in_reply_to, forward.references.len()), ("", None, 0));
    assert_eq!((forward.focus, forward.files.len()), (Field::To, 0), "the message's files are not carried over");
}

#[test]
fn a_draft_opens_as_it_was_saved_and_an_agent_s_is_marked() {
    let ids = fx::identities();
    let draft = opening(&ids).from_draft(&fx::agent_draft());
    assert_eq!(draft.key, "draft-agent");
    assert_eq!(draft.stored, Some(fx::agent_draft().envelope.id));
    assert_eq!(draft.to, "Priya Raman <priya@example.org>, Dmitri Volkov <dmitri@example.org>");
    assert_eq!(draft.cc, "billing@halden.example.com");
    assert_eq!(draft.in_reply_to.as_deref(), Some("<halden-1@design.example.net>"));
    assert_eq!(
        draft.files.iter().map(|f| (f.name.as_str(), f.path.is_none())).collect::<Vec<_>>(),
        [("contract-v3-signed.pdf", true), ("schedule-b.xlsx", true)]
    );
    assert_eq!(draft.agent.as_ref().map(|mark| stamp(mark.at)).as_deref(), Some("23 Nov 13:52"));
    assert_eq!(draft.draft(&ids).unwrap().origin, Origin::Agent, "and stays an agent's when handed on (ADR-016)");
    let own = opening(&ids).from_draft(&fx::own_reply("Hi"));
    assert_eq!(
        (own.agent, own.body.as_str(), ids[own.identity].address.email.as_str()),
        (None, "Hi", "robin.vale@example.org")
    );
    let mut blind = fx::own_reply("Hi");
    blind.envelope.bcc = vec![Address { name: None, email: "audit@example.org".into() }];
    assert_eq!(opening(&ids).from_draft(&blind).bcc, "audit@example.org", "a blind copy is still on the draft");
}

#[test]
fn a_mailto_link_fills_the_fields_it_may_and_no_other() {
    let ids = fx::identities();
    let work = AccountId("work".into());
    let url = "mailto:list@example.net,%20owner@example.net?Subject=un%73ubscribe%0D%0ABcc:%20x@example.com&cc=a@example.org&bcc=eve@example.net&body=line%20one%0Aline%20two&x-header=1";
    let c = opening(&ids).mailto(Some(&work), url).unwrap();
    assert_eq!(c.to, "list@example.net, owner@example.net");
    assert_eq!(c.subject, "unsubscribe Bcc: x@example.com", "no line break reaches the subject");
    assert_eq!((c.cc.as_str(), c.bcc.as_str(), c.body.as_str()), ("a@example.org", "", "line one\nline two"));
    assert_eq!(ids[c.identity].account, work);
    assert!(c.dirty, "what a link wrote is asked about before it is thrown away");
    let odd = opening(&ids).mailto(None, "MAILTO:not an address,ok@example.org").unwrap();
    assert_eq!(odd.to, "ok@example.org");
    assert!(opening(&ids).mailto(None, "https://example.org/").is_none());
}

/// A name from a message is typed into a field only in a form the field
/// reads back as that one address.
#[test]
fn an_address_is_written_so_that_it_reads_back() {
    let table = [
        (person("Priya Raman", "priya@example.org"), "Priya Raman <priya@example.org>"),
        (person("Raman, Priya", "priya@example.org"), "\"Raman, Priya\" <priya@example.org>"),
        (person("", "priya@example.org"), "priya@example.org"),
        (person("Eve <eve@example.net>", "priya@example.org"), "priya@example.org"),
        (person("Eve\r\nBcc: eve@example.net", "priya@example.org"), "priya@example.org"),
        (person("\"quoted\"", "priya@example.org"), "priya@example.org"),
    ];
    for (address, want) in table {
        assert_eq!(written(&address), want);
        assert_eq!(tuimex_core::addresses(want).unwrap().len(), 1, "{want}");
    }
}

#[test]
fn the_thread_answered_is_found_by_message_id_without_drafts_or_the_outbox() {
    let mut mail = fx::inbox();
    mail.extend([fx::agent_draft(), fx::own_reply("")]);
    let work = AccountId("work".into());
    let keys = |found: Vec<&Listed>| found.iter().map(|m| m.envelope.id.key.clone()).collect::<Vec<_>>();
    assert_eq!(keys(thread(&mail, &work, Some("<halden-1@design.example.net>"))), ["halden-1", "halden-2", "halden-3"]);
    assert_eq!(keys(thread(&mail, &work, Some("halden-3@design.example.net"))), ["halden-1", "halden-2", "halden-3"]);
    assert_eq!(keys(thread(&mail, &work, None)), [] as [&str; 0]);
    assert_eq!(keys(thread(&mail, &work, Some("<nothing@example.net>"))), [] as [&str; 0]);
    let personal = AccountId("personal".into());
    assert_eq!(
        keys(thread(&mail, &personal, Some("<halden-1@design.example.net>"))),
        [] as [&str; 0],
        "another account's"
    );
}

/// ADR-017: what the agent is given is the reader's text, under headers.
#[test]
fn the_conversation_for_the_agent_is_each_message_s_headers_and_reader_text() {
    let mail = fx::inbox();
    let found = thread(&mail, &AccountId("work".into()), Some("<halden-1@design.example.net>"));
    let said = conversation(&found, &person("Robin Vale", "robin.vale@example.org"));
    assert!(said.starts_with("The reply will be sent by Robin Vale <robin.vale@example.org>.\n\nFrom: Priya Raman <priya@example.org>\nTo: Robin Vale <robin.vale@example.org>\nCc: Dmitri Volkov <dmitri@example.org>\nDate: 23 Nov 09:14\nSubject: Contract draft v3 for the Halden account\n\nHi Robin,\n\nAttached is the third draft"), "{said}");
    assert!(said.contains(mail.iter().find(|m| m.envelope.id.key == "halden-3").unwrap().text.as_deref().unwrap()));
    assert!(
        said.contains("\n\nFine with being a named contact. 45 days works for me.\n"),
        "one not read yet is its opening"
    );
    assert_eq!(
        names(&found, &[person("Studio desk", "desk@example.org")]),
        ["Studio desk", "Priya Raman", "Robin Vale", "Dmitri Volkov"]
    );
}

fn summary_of(c: &Compose, mail: &[Listed], signing: &Signing) -> Vec<String> {
    let ids = fx::identities();
    let draft = c.draft(&ids).unwrap();
    let found = thread(mail, &draft.id.account, c.in_reply_to.as_deref());
    let right = list::alerts(&list::Sync::default());
    let o = Summary { draft: &draft, agent: c.agent.as_ref(), thread: &found, signing, delay: 10, right: &right };
    text(&summary::screen(&o, 120, 30))
}

/// PLAN §6.4: a domain that appears nowhere in the thread is marked.
#[test]
fn a_recipient_from_outside_the_thread_is_marked_by_registrable_domain() {
    let ids = fx::identities();
    let mut c = opening(&ids).from_draft(&fx::own_reply("Hi"));
    c.to = "Priya Raman <priya@example.org>, pay@mail.example.net, someone-with-a-very-long-address-indeed@elsewhere.example.com".into();
    c.cc = "Ada <ada@example.com>".into();
    c.bcc = "audit@dept.example.org".into();
    let rows = summary_of(&c, &fx::inbox(), &Signing::Neither);
    let want = [
        "    From     Robin Vale <robin.vale@example.org>",
        "    To       Priya Raman <priya@example.org>",
        "             pay@mail.example.net",
        "             someone-with-a-very-long-address-indeed@elsewhere.example.com  ▲ not in this thread",
        "    Cc       Ada <ada@example.com>                         ▲ not in this thread",
        "    Bcc      audit@dept.example.org",
        "    Subject  Re: Contract draft v3 for the Halden account",
        "    Signing  not signed · not encrypted",
        "",
    ];
    assert_eq!(rows[2..11], want);
    assert_eq!(rows[29], "  Send to 5 recipients?  y send, held 10s · n back");
    // A new message answers no thread, so nobody is outside one.
    c.in_reply_to = None;
    assert!(!summary_of(&c, &fx::inbox(), &Signing::Neither).join("\n").contains("not in this thread"));
}

/// Both accounts are the user's, and a message from one to the other is
/// not therefore "one's own": answered as if it were, the reply went back
/// to the account it was read in.
#[test]
fn a_reply_to_mail_from_another_of_ones_own_accounts_goes_to_that_account() {
    let ids = fx::identities();
    let (work, personal) = (ids.iter().find(|i| i.account.0 == "work").unwrap(), &ids[0]);
    assert_ne!(work.account, personal.account);
    let mut received = mail("migration");
    assert_eq!(received.envelope.id.account, work.account, "read in the work account");
    received.envelope.from = vec![personal.address.clone()];
    received.envelope.to = vec![work.address.clone()];
    let reply = opening(&ids).reply(&received, false);
    assert_eq!(reply.to, written(&personal.address));
    assert_eq!(ids[reply.identity].address, work.address, "from the account that received it");
    // What the account itself sent is still answered to whom it went.
    let mut sent = received.clone();
    (sent.envelope.from, sent.envelope.to) = (vec![work.address.clone()], vec![person("Ada", "ada@example.com")]);
    assert_eq!(opening(&ids).reply(&sent, false).to, "Ada <ada@example.com>");
}

/// As mutt answers: `Reply-To` is where the sender asked for answers. It
/// is also how a reply is led away from its thread, so the summary judges
/// the address by the thread's other headers and not by the header that
/// named it.
#[test]
fn a_reply_goes_where_reply_to_asks_and_the_summary_still_marks_a_domain_the_thread_never_named() {
    let ids = fx::identities();
    let asking = |address: &str| {
        let mut m = mail("halden-3");
        m.envelope.reply_to = vec![person("Priya Raman", address)];
        m
    };
    let inbox = |asking: &Listed| -> Vec<Listed> {
        fx::inbox().into_iter().map(|m| if m.envelope.id == asking.envelope.id { asking.clone() } else { m }).collect()
    };
    let marked = |rows: &[String], address: &str| {
        rows.iter().any(|row| row.contains(address) && row.ends_with("▲ not in this thread"))
    };
    let (desk, elsewhere) = (asking("desk@example.org"), asking("priya@example.com"));
    let within = opening(&ids).reply(&desk, false);
    assert_eq!(within.to, "Priya Raman <desk@example.org>");
    assert!(!marked(&summary_of(&within, &inbox(&desk), &Signing::Neither), "desk@example.org"));
    let redirected = opening(&ids).reply(&elsewhere, true);
    assert_eq!(redirected.to, "Priya Raman <priya@example.com>");
    assert!(!redirected.cc.contains("priya@"), "the sender is answered once, where it asked: {}", redirected.cc);
    let rows = summary_of(&redirected, &inbox(&elsewhere), &Signing::Neither);
    assert!(marked(&rows, "priya@example.com"), "{rows:#?}");
}

#[test]
fn the_summary_says_who_wrote_the_body_and_totals_the_files() {
    let ids = fx::identities();
    let agent = opening(&ids).from_draft(&fx::agent_draft());
    let rows = summary_of(&agent, &fx::inbox(), &Signing::Neither);
    assert_eq!(rows[9], "             total                        550 KB");
    assert_eq!(rows[12], "    ◇ Written by an agent · 23 Nov 13:52 · files added by you");
    let mut own = opening(&ids).from_draft(&fx::own_reply("Hi"));
    assert!(!summary_of(&own, &fx::inbox(), &Signing::Neither).join("\n").contains("agent"));
    own.agent = Some(super::Mark { at: fx::NOW, tidied: Some(vec!["Hi".into()]) });
    let rows = summary_of(&own, &fx::inbox(), &Signing::Neither);
    assert_eq!(
        rows[7..9],
        ["", "    ◇ Tidied by an agent · 23 Nov 14:20"],
        "one recipient to a line, no file, no total"
    );
}

#[test]
fn a_field_shows_a_recipient_to_a_line_and_what_does_not_parse_as_typed() {
    let ids = fx::identities();
    let mut c = Compose::new("k".into(), 1);
    c.to = "Priya Raman <priya@example.org>, dmitri@, ".into();
    c.subject = "Lunch".into();
    c.body = "one\n\ntwo\n\n\n\nthree".into();
    let view = super::View { identities: &ids, left: None, right: Line::new(), command: None, panel: Vec::new() };
    let rows = text(&c.screen(&view, 60, 20));
    let want = [
        "  Lunch",
        "",
        "    From     robin@example.com   robin.vale@example.org   d…",
        "▌   To       Priya Raman <priya@example.org>",
        "             dmitri@",
        "",
        "    Cc",
        "    Bcc",
        "    Subject  Lunch",
        "",
        "    one",
        "",
        "    two",
        "",
        "",
        "    three",
    ];
    assert_eq!(rows[..16], want);
    c.focus = Field::Body;
    assert_eq!(
        text(&c.screen(&view, 60, 20))[3..5],
        ["    To       Priya Raman <priya@example.org>", "             dmitri@"]
    );
    assert_eq!(text(&Compose::new("k".into(), 0).screen(&view, 60, 20))[0], "  New message");
    assert_eq!(c.recipients().unwrap_err(), "\"dmitri@\" is not an address");
}
