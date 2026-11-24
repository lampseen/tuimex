//! What the reader shows of the design's mailbox beyond a row and its
//! preview: `NEWSLETTER_BODY`, `NEWSLETTER_LINKS`, `BANK_LINKS`, the
//! invitation's card, the photographs' inline picture, and what `gpg` is
//! taken to have said of each signed or encrypted message.

use tuimex_core::{Body, Check, EventTime, Invite, Link, Listed, Picture, Unsubscribe};

use super::{DMITRI, IMANI, LARS, ME, ME_WORK, MEI, OLU, PRIYA, address, at};

/// `NEWSLETTER_BODY`, as the reader's text says a heading, a picture's alt
/// text, a table, a quote, a list and the small print under a rule.
const NEWSLETTER: &str = "# Fernway Weekly, 23 November\n\n\
[image: Ridge loop at sunrise]\n\n\
Winter routes are open. Three of the five marked loops are groomed and the hut at Ridge saddle is staffed at \
weekends. Route notes and maps [1]\n\n\
# Route status\n\n\
Route\tLength\tStatus\nRidge loop\t12 km\tgroomed\nSaddle traverse\t18 km\tgroomed\nMill creek\t7 km\tclosed until 5 Dec\n\n\
# Gear swap, 5 December\n\n\
Bring what you have outgrown and take home what fits. Sign up for a table [2]\n\n\
> We cleared forty pairs of skis last year and nobody went home with what they brought. (Marta, swap organiser)\n\n\
* Doors at 10:00\n* Cash or card\n* Leftovers go to the school club\n\n\
Mill creek is closed for bridge work. Details and the detour [3]\n\n\
---\n\n\
You get this because you joined Fernway in 2023. Unsubscribe [4] or view it in a browser [5]";

/// `MIGRATION_BODY`: long enough to scroll in the preview at any size.
pub(super) const MIGRATION: &str = "Hi Robin,\n\n\
Here is the plan for moving the staging database to the new cluster. Nothing in it is decided until you have read it, \
and the window itself is the part I most need your view on.\n\n\
# The window\n\n\
I would like Thursday from 20:00 to 23:00. Staging is quiet then, the nightly backup has not started, and the two of us \
who know the old cluster are both free. Friday is the fallback, though a Friday change leaves the weekend to find out \
what we missed.\n\n\
# Before the window\n\n\
* Freeze schema changes on staging from Wednesday noon\n\
* Take a full dump and restore it on the new cluster as a rehearsal\n\
* Compare row counts and checksums table by table\n\
* Point one internal service at the rehearsal copy for a day\n\n\
The rehearsal matters more than the rest. Last time the restore itself was fine and the surprise was a collation \
difference that changed the order of two reports. If the checksums match and the reports read the same, I am \
comfortable.\n\n\
# During the window\n\n\
* 20:00 stop writers and take the final dump\n\
* 20:20 restore on the new cluster\n\
* 21:10 run the comparison again\n\
* 21:30 switch the connection strings and restart the services\n\
* 22:00 smoke tests, then an hour of watching\n\n\
If the comparison at 21:10 fails we stop there, start the writers again on the old cluster and try another night. \
Nothing has been switched at that point, so going back costs ten minutes.\n\n\
# After\n\n\
The old cluster stays up, read-only, for a week. After that I will ask you before it is removed. The backup job needs \
its target changed the same night, or the first nightly run will dump an idle database and report success.\n\n\
# What I need from you\n\n\
A yes or no on Thursday, and a second pair of eyes on the comparison script before Wednesday. It is short, and I would \
rather you found the hole in it than staging did.\n\n\
Thanks,\nLars";

const PHOTOS: &str = "Here are the ones that came out. The first is the view from the saddle; the full set follows \
once I have sorted them.\n\n[image: ridge-01.jpg]\n\nG.";

fn links(all: &[(&str, &str)]) -> Vec<Link> {
    let link =
        |(i, (url, shown)): (usize, &(&str, &str))| Link { n: i + 1, url: url.to_string(), shown: shown.to_string() };
    all.iter().enumerate().map(link).collect()
}

/// The addresses the design's two accounts receive mail at.
pub fn addresses() -> Vec<String> {
    vec![ME.1.to_string(), ME_WORK.1.to_string()]
}

/// What reading `msg` gives: its text, and for the messages the design
/// draws more of, the rest.
pub fn body(msg: &Listed) -> Body {
    let text = msg.text.clone().unwrap_or_else(|| msg.snippet.clone());
    let plain = Body {
        text,
        links: Vec::new(),
        attachments: msg.attachments.clone(),
        invite: None,
        blocked_remote: Vec::new(),
        remote_images: 0,
        image_hosts: Vec::new(),
        loadable_images: 0,
        trackers: Vec::new(),
        unsubscribe: None,
        pictures: Vec::new(),
    };
    match msg.envelope.id.key.as_str() {
        "newsletter" => Body {
            text: NEWSLETTER.to_string(),
            links: links(&[
                (
                    "https://fernway.example.net/routes/winter?utm_source=weekly&utm_medium=email",
                    "Route notes and maps",
                ),
                ("https://shop.example.net/swap/tables", "Sign up for a table"),
                ("https://fernway.example.net/closures/mill-creek", "Details and the detour"),
                ("https://fernway.example.net/unsubscribe", "Unsubscribe"),
                ("https://click.opentrack.example.net/v/8f3a21", "view it in a browser"),
            ]),
            blocked_remote: ["img.fernway.example.net", "cdn.example.net", "click.opentrack.example.net"]
                .map(String::from)
                .into(),
            remote_images: 9,
            // The design's `loadable(remote)`: the tracker's host and its
            // one image are not what a yes loads.
            image_hosts: ["img.fernway.example.net", "cdn.example.net"].map(String::from).into(),
            loadable_images: 8,
            trackers: vec!["OpenTrack".to_string()],
            unsubscribe: Some(Unsubscribe::Https("https://fernway.example.net/unsubscribe".to_string())),
            ..plain
        },
        "bank" => Body {
            links: links(&[
                ("https://bank.example.com.device-check.example.net/confirm", "https://bank.example.com/confirm"),
                ("https://examplebank.example.net/contact", "Contact us"),
            ]),
            ..plain
        },
        "invite" => Body {
            links: links(&[("https://meet.example.org/r/q4-roadmap", "the call link")]),
            invite: Some(Invite {
                uid: "q4-roadmap@design.example.net".to_string(),
                summary: "Q4 roadmap review".to_string(),
                start: EventTime::At(at(26, 10, 0)),
                end: Some(EventTime::At(at(26, 11, 30))),
                location: Some("Room Fjord, or the call link [1]".to_string()),
                organizer: Some(address(IMANI)),
                attendees: [ME_WORK, LARS, MEI, DMITRI].map(address).into(),
                recurrence: None,
            }),
            ..plain
        },
        "photos" => {
            Body { text: PHOTOS.to_string(), pictures: vec![Picture { index: 0, width: 1600, height: 1067 }], ..plain }
        }
        _ => plain,
    }
}

/// The design's `security` of a message, as `gpg` would have reported it.
pub fn check(key: &str) -> Option<Check> {
    Some(match key {
        "halden-1" => Check::Good { signer: PRIYA.0.to_string(), key: "7A1C55E0".to_string() },
        "vault" => Check::Bad { signer: OLU.1.to_string() },
        "payroll" => Check::Decrypted { key: "51F09A2C".to_string() },
        _ => return None,
    })
}
