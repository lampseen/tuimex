//! The corpus's link and attachment cases through the UI (PLAN §10, M8):
//! each message is opened in the real `App`, the keys a person would type
//! are typed, and what the app then asks for is carried out for real
//! under a scratch root, with a list standing in for the system's opener.
//! Nothing here can start a browser or a viewer.

use std::cell::RefCell;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, SystemTime};

use tuimex_core::{
    AccountId, Action, Ask, Body, Envelope, Event, Flags, Folder, FolderId, Listed, MessageId, Origin, Query, Role,
    Section, Sort, Timestamp,
};
use tuimex_ui::app::App;
use tuimex_ui::glyphs::Glyphs;
use tuimex_ui::keymap::Key;
use tuimex_ui::mailbox::Mailbox;
use tuimex_ui::paint::Theme;
use tuimex_ui::palette::{Palette, Tier};

use super::{Opener, Outside, homely, read};
use crate::paths::Dirs;

struct Recorded(Rc<RefCell<Vec<String>>>);

impl Opener for Recorded {
    fn open(&mut self, target: &str) -> std::io::Result<()> {
        self.0.borrow_mut().push(target.to_string());
        Ok(())
    }
}

/// One corpus message, alone in an inbox, with the app over it.
struct Session {
    app: App,
    outside: Option<Outside>,
    raw: Vec<u8>,
    body: Body,
    opened: Rc<RefCell<Vec<String>>>,
    root: PathBuf,
}

fn id() -> MessageId {
    MessageId { account: AccountId("test".into()), key: "one".into() }
}

/// A path as tuimex says it on this machine. The scratch root is under
/// the home directory wherever the checkout is, as it is on a CI runner,
/// and a path under home is said from a tilde.
fn shown(path: &Path) -> String {
    let home = directories::BaseDirs::new().map(|dirs| dirs.home_dir().to_path_buf());
    homely(path, home.as_deref())
}

fn session(file: &str, test: &str) -> Session {
    let corpus = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus").join(file);
    let raw = std::fs::read(&corpus).unwrap();
    let (head, body) = (tuimex_content::envelope(&raw).unwrap(), read(&raw).unwrap());
    let inbox = FolderId("Inbox".into());
    let envelope = Envelope {
        id: id(),
        folder: inbox.clone(),
        date: head.date.unwrap_or(Timestamp(0)),
        from: head.from,
        to: head.to,
        cc: head.cc,
        bcc: Vec::new(),
        reply_to: Vec::new(),
        subject: head.subject,
        message_id: head.message_id,
        references: head.references,
        flags: Flags::default(),
        size: 0,
        has_attachments: head.has_attachments,
        security: head.security,
        origin: Origin::Server,
    };
    let listed = Listed {
        envelope,
        snippet: String::new(),
        attachments: body.attachments.clone(),
        invite: false,
        unsubscribe: false,
        text: None,
        sender_was: None,
        sending: None,
    };
    let section = Section {
        name: "Inbox".into(),
        short: "Inbox".into(),
        query: Query::parse("in:inbox"),
        sort: Sort::NewestFirst,
        threaded: true,
    };
    let folder =
        Folder { id: inbox, account: id().account, name: "Inbox".into(), role: Role::Inbox, unseen: 1, total: 1 };
    let mailbox = Mailbox {
        now: Timestamp(1_795_443_600),
        sections: vec![section],
        folders: vec![folder],
        mail: vec![listed],
        synced: None,
        me: Vec::new(),
    };
    // Under test this is a scratch root beneath the build directory.
    let root = Dirs::from_env().unwrap().cache.join(test).join(file);
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    let opened = Rc::new(RefCell::new(Vec::new()));
    let outside = Outside::new(Box::new(Recorded(opened.clone())), root.join("downloads"), &root.join("cache"));
    let app = App::new(Theme { palette: Palette::new(Tier::True), glyphs: Glyphs::Unicode }, mailbox);
    Session { app, outside: Some(outside), raw, body, opened, root }
}

impl Session {
    /// Types `keys`, a return being Enter, and after each carries out what
    /// the app asked for.
    fn keys(&mut self, keys: &str) -> &mut Self {
        for c in keys.chars() {
            self.app.key(if c == '\r' { Key::Enter } else { Key::Char(c) });
            for ask in self.app.asks() {
                let told = match ask.action {
                    Action::Read => Some(Event::BodyReady { id: id(), body: Box::new(self.body.clone()) }),
                    _ => self.outside.as_mut().unwrap().carry_out(&ask, &self.body, Some(&self.raw)),
                };
                told.into_iter().for_each(|event| self.app.event(event));
            }
        }
        self
    }

    fn screen(&mut self) -> Vec<String> {
        let lines = self.app.lines(160, 40);
        lines.iter().map(|l| l.iter().map(|s| s.text.as_str()).collect::<String>().trim().to_string()).collect()
    }

    /// The command line, on a screen wide enough that a path under the
    /// scratch root is said whole.
    fn said(&mut self) -> String {
        let line = self.app.lines(400, 40).pop().unwrap();
        line.iter().map(|s| s.text.as_str()).collect::<String>().trim().to_string()
    }

    fn opened(&self) -> Vec<String> {
        self.opened.borrow().clone()
    }

    /// Every file under the scratch root, and its mode.
    fn written(&self) -> Vec<(PathBuf, u32)> {
        fn walk(dir: &Path, out: &mut Vec<(PathBuf, u32)>) {
            for entry in std::fs::read_dir(dir).into_iter().flatten().map(Result::unwrap) {
                let meta = entry.metadata().unwrap();
                if meta.is_dir() {
                    assert_eq!(meta.permissions().mode() & 0o777, 0o700, "{}", entry.path().display());
                    walk(&entry.path(), out);
                } else {
                    out.push((entry.path(), meta.permissions().mode() & 0o777));
                }
            }
        }
        let mut out = Vec::new();
        walk(&self.root, &mut out);
        out
    }
}

/// ADR-018: a link that is neither web nor mail is in the list, marked,
/// and no key or command opens it; nor does asking for it behind the UI's
/// back.
#[test]
fn a_link_of_another_scheme_is_shown_and_refused() {
    for (file, shown, kind) in [
        ("link-scheme-file.eml", "file:", "file links are"),
        ("link-scheme-ssh.eml", "ssh:git.example.net", "ssh links are"),
        ("link-scheme-javascript.eml", "javascript:", "javascript links are"),
        ("data-url-not-an-image.eml", "data:", "data links are"),
        ("base-href.eml", "(no host)", "a link to no host is"),
    ] {
        let mut s = session(file, "refused");
        let row = s.keys("\ro").screen().into_iter().find(|row| row.contains(" 1  ")).unwrap();
        assert!(row.starts_with(&format!("▌    1  {shown}  ")) && row.ends_with("▲ will not open"), "{file}: {row}");
        for keys in ["1", "\r", ":link 1\r"] {
            let said = s.keys(keys).said();
            let want = format!("E :link 1: {kind} not opened · only http, https and mailto");
            assert_eq!(said, want, "{file} {keys:?}");
        }
        let behind = Ask { serial: 99, action: Action::OpenLink(1), on: vec![id()] };
        let told = s.outside.as_mut().unwrap().carry_out(&behind, &s.body, Some(&s.raw));
        assert!(matches!(told, Some(Event::ActionFailed { serial: 99, .. })), "{file}: {told:?}");
        assert!(s.opened().is_empty(), "{file}: {:?}", s.opened());
    }
}

/// ADR-018: where a link really goes is said before it opens, and one
/// that deceives opens only on `y`.
#[test]
fn a_link_that_deceives_is_confirmed_first_and_names_its_real_host() {
    for (file, warning, host, url) in [
        (
            "link-text-names-another-host.eml",
            "Text says www.example.com",
            "login.example.net",
            "https://login.example.net/session",
        ),
        (
            "link-credentials-before-host.eml",
            "Address carries a sign-in name",
            "login.example.net",
            "https://www.example.com@login.example.net/session",
        ),
        ("link-bare-ip.eml", "Host is a bare IP address", "192.0.2.7", "http://192.0.2.7/session"),
        (
            "link-mixed-script.eml",
            "Host mixes alphabets",
            "p\u{430}ypal.example.net",
            "https://p\u{430}ypal.example.net/session",
        ),
    ] {
        let mut s = session(file, "confirmed");
        let asked = s.keys("\ro1").said();
        assert_eq!(asked, format!("▲ {warning}; opens {host}. Open?  y yes · n no"), "{file}");
        assert!(s.opened().is_empty(), "{file}: opened before the answer");
        for no in ["n", "x", "o\ry"] {
            assert!(s.keys(no).opened().is_empty(), "{file}: {no:?} opened it");
            s.keys("o1");
        }
        assert_eq!(s.keys("y").opened(), [url], "{file}");
    }
}

#[test]
fn an_honest_link_opens_at_once_without_its_tracking_parameters() {
    let mut s = session("link-tracking-parameters.eml", "honest");
    assert!(s.keys("\ro").screen().iter().any(|row| row.ends_with("See the offer   tracking removed")));
    assert_eq!(s.keys("1").opened(), ["https://www.example.com/offer?id=7"]);
    assert_eq!(s.said(), "Opening www.example.com");
    let mut s = session("plain-text-links.eml", "honest");
    assert_eq!(s.keys("\r:link 2\r").opened(), ["http://archive.example.org/notes?v=1"]);
}

/// ADR-007, ADR-011: whatever a sender named a file, it is saved under a
/// name that is only a name, inside the download directory, owner-only,
/// and a second save does not write over the first.
#[test]
fn a_file_is_saved_under_a_safe_name_inside_the_download_directory() {
    for (file, name) in [
        ("attachment-path-traversal.eml", "authorized_keys"),
        ("attachment-absolute-path.eml", "backup"),
        ("attachment-windows-path.eml", "run.bat"),
        ("attachment-encoded-traversal.eml", "bashrc"),
        ("attachment-dotfile.eml", "profile"),
        ("attachment-option-name.eml", "rf"),
        ("attachment-rtl-override.eml", "invoicefdp.exe"),
        ("attachment-escape-in-name.eml", "report[2J.pdf"),
        ("attachment-no-name.eml", "attachment"),
        ("attachment-mime-not-a-type.eml", "notes.txt"),
    ] {
        let mut s = session(file, "saved");
        let downloads = s.root.join("downloads");
        assert_eq!(s.keys("\ra\r").said(), format!("✓ Saved {}", shown(&downloads.join(name))), "{file}");
        assert_eq!(s.written(), [(downloads.join(name), 0o600)], "{file}");
        let data = tuimex_content::attachment(&s.raw, 0).unwrap().data;
        assert_eq!(std::fs::read(downloads.join(name)).unwrap(), data, "{file}");

        s.keys("a\r");
        let second = match name.rsplit_once('.') {
            Some((stem, extension)) => format!("{stem}-2.{extension}"),
            None => format!("{name}-2"),
        };
        let mut written: Vec<PathBuf> = s.written().into_iter().map(|(path, _)| path).collect();
        written.sort();
        let mut both = [downloads.join(name), downloads.join(&second)];
        both.sort();
        assert_eq!(written, both, "{file}");
        assert!(s.opened().is_empty(), "{file}: saving opens nothing");
    }
}

#[test]
fn a_path_given_to_save_is_read_against_the_download_directory_and_never_written_over() {
    let mut s = session("attachment-path-traversal.eml", "path");
    let to = s.root.join("downloads/keys/mine.pub");
    assert_eq!(s.keys("\r:save 1 keys/mine.pub\r").said(), format!("✓ Saved {}", shown(&to)));
    assert_eq!(s.keys(":save 1 keys/mine.pub\r").said(), "E :save: mine.pub is there already");
    assert_eq!(
        s.keys(":save 1 keys\r").said(),
        format!("✓ Saved {}", shown(&s.root.join("downloads/keys/authorized_keys")))
    );
    assert_eq!(s.keys(":save 2\r").said(), "E :save: this message has no file 2 · a lists them");
    assert_eq!(s.written().len(), 2);
}

/// PLAN §10, 0.1.0: a file opens in the system's program for its type,
/// after a question that names the type. What is handed over is a private
/// copy under the cache directory that goes when tuimex does (ADR-011).
#[test]
fn a_file_is_opened_from_a_private_copy_after_asking_and_removed_at_exit() {
    let mut s = session("attachment-mime-not-a-type.eml", "opened");
    let asked = s.keys("\raO").said();
    assert_eq!(asked, "Open notes.txt (application/octet-stream) in the system's program for it?  y yes · n no");
    assert!(s.keys("n").opened().is_empty() && s.written().is_empty());
    let opened = s.keys("aOy").opened();
    let [copy] = opened.as_slice() else { panic!("{opened:?}") };
    assert!(Path::new(copy).starts_with(s.root.join("cache/shown")) && copy.ends_with("/notes.txt"), "{copy}");
    assert_eq!(s.written(), [(PathBuf::from(copy), 0o600)]);
    s.outside = None;
    assert!(s.written().is_empty(), "the copy outlived the run");
}

/// ADR-009: the browser is handed the sanitised page, sealed. A message
/// that names a host and no picture on it is not asked about.
#[test]
fn the_browser_copy_is_the_sealed_page_written_owner_only_and_removed_at_exit() {
    let mut s = session("script-in-html.eml", "browser");
    let opened = s.keys("\rB").opened();
    let [copy] = opened.as_slice() else { panic!("{opened:?}: {}", s.said()) };
    assert!(Path::new(copy).starts_with(s.root.join("cache/shown")) && copy.ends_with("/message.html"), "{copy}");
    assert_eq!(s.written(), [(PathBuf::from(copy), 0o600)]);
    let page = std::fs::read_to_string(copy).unwrap();
    assert!(page.contains("default-src 'none'") && !page.contains("collect.example.net"), "{page}");
    assert!(!page.contains("<script") && !page.contains("x.js"), "{page}");

    assert_eq!(s.said(), "Opening the sealed copy in your browser · nothing remote loads");
    s.outside = None;
    assert!(s.written().is_empty(), "a copy outlived the run");

    let mut plain = session("plain-text-links.eml", "browser");
    assert_eq!(plain.keys("\rB").said(), "E :browser: this message has no HTML to show");
    assert!(plain.opened().is_empty());
}

/// The policy the copy the opener was last handed begins with.
fn policy(s: &Session) -> String {
    let page = std::fs::read_to_string(s.opened().pop().unwrap()).unwrap();
    let from = page.find("content=\"").unwrap() + "content=\"".len();
    page[from..].split('"').next().unwrap().to_string()
}

/// ADR-009: `B` on a message whose pictures are on a server asks each
/// time, and a yes lets the copy load pictures from the hosts named, over
/// https, and nothing else.
#[test]
fn a_yes_to_the_pictures_allows_exactly_the_hosts_named_for_images_over_https() {
    let mut s = session("tracker-pixel-beside-pictures.eml", "browser-yes");
    let asked = s.keys("\rB").said();
    assert_eq!(asked, "Load 2 images from img.example.net, photos.example.org?  y yes · n no");
    assert!(s.opened().is_empty() && s.written().is_empty(), "nothing is written before the answer");
    assert_eq!(s.keys("y").said(), "Opening the sealed copy in your browser · 2 images load from 2 hosts");
    assert_eq!(s.opened().len(), 1);
    assert_eq!(
        policy(&s),
        "default-src 'none'; img-src data: https://img.example.net https://photos.example.org; \
         style-src 'unsafe-inline'"
    );
    assert!(s.keys("B").said().starts_with("Load 2 images "), "a yes is not remembered");
    assert_eq!(s.opened().len(), 1);
}

/// A yes to pictures is not a yes to being counted: an image on a
/// tracking service's host, a pixel across, of no size or not displayed
/// is not asked about, not allowed and not in the copy, on a host of its
/// own or on one that also serves a picture. With nothing else to load
/// there is no question.
#[test]
fn a_yes_to_the_pictures_loads_no_tracker_and_a_message_with_only_trackers_is_not_asked() {
    for file in ["tracker-pixel.eml", "tracker-hidden-image.eml", "tracker-known-service.eml"] {
        let mut s = session(file, "browser-trackers-alone");
        assert_eq!(s.keys("\rB").said(), "Opening the sealed copy in your browser · nothing remote loads", "{file}");
        assert_eq!(policy(&s), "default-src 'none'; img-src data:; style-src 'unsafe-inline'", "{file}");
    }
    for (file, asked, kept, never) in [
        ("tracker-pixel-beside-pictures.eml", "Load 2 images from img.example.net, photos.example.org?", 2, "open."),
        ("tracker-pixel-on-a-picture-host.eml", "Load 1 image from img.example.net?", 1, "abc.gif"),
        ("tracker-zero-size-and-undisplayed.eml", "Load 1 image from img.example.net?", 1, "/o/"),
    ] {
        let mut s = session(file, "browser-trackers-beside");
        assert_eq!(s.keys("\rB").said(), format!("{asked}  y yes · n no"), "{file}");
        s.keys("y");
        let page = std::fs::read_to_string(s.opened().pop().unwrap()).unwrap();
        assert_eq!(page.matches("src=\"https://").count(), kept, "{file}: {page}");
        assert!(!page.contains(never), "{file}: {never} is in {page}");
    }
}

/// A stylesheet, a script and a font are never loaded, whatever the
/// answer, so their hosts are not what the question is about; nor is a
/// picture the copy would drop for how it is addressed.
#[test]
fn the_question_names_and_counts_only_the_pictures_a_yes_loads_among_a_script_and_a_stylesheet_host() {
    let mut s = session("remote-images-among-other-hosts.eml", "browser-image-hosts");
    assert_eq!(s.body.blocked_remote.len(), 7, "the message names seven hosts in all");
    let asked = s.keys("\rB").said();
    assert_eq!(asked, "Load 3 images from img.example.net, photos.example.org?  y yes · n no");
    assert_eq!(s.keys("y").said(), "Opening the sealed copy in your browser · 3 images load from 2 hosts");
    assert_eq!(
        policy(&s),
        "default-src 'none'; img-src data: https://img.example.net https://photos.example.org; \
         style-src 'unsafe-inline'"
    );
    let page = std::fs::read_to_string(s.opened().pop().unwrap()).unwrap();
    assert_eq!(page.matches("src=\"https://").count(), 3, "{page}");
    for never in ["styles.example.net", "scripts.example.net", "fonts.example.net", "plain.example.net", "keyed."] {
        assert!(!page.contains(never), "{never} is in {page}");
    }
}

#[test]
fn a_no_to_the_pictures_opens_the_copy_sealed() {
    let mut s = session("tracker-pixel-beside-pictures.eml", "browser-no");
    assert_eq!(s.keys("\rBn").said(), "Opening the sealed copy in your browser · nothing remote loads");
    assert_eq!(s.opened().len(), 1);
    assert_eq!(policy(&s), "default-src 'none'; img-src data:; style-src 'unsafe-inline'");
    let page = std::fs::read_to_string(s.opened().pop().unwrap()).unwrap();
    assert!(!page.contains("example.net") && !page.contains("example.org"), "{page}");
}

/// Esc leaves the question, as it leaves every other, and nothing opens.
#[test]
fn leaving_the_question_about_pictures_opens_nothing() {
    let mut s = session("tracker-pixel-beside-pictures.eml", "browser-esc");
    s.keys("\rB");
    s.app.key(Key::Esc);
    assert!(s.app.asks().is_empty() && s.opened().is_empty() && s.written().is_empty());
}

/// A picture named only in a `srcset` is one the copy never keeps, so
/// there is nothing a yes could load.
#[test]
fn a_message_with_no_remote_pictures_opens_without_a_question() {
    for file in ["data-url-image.eml", "inline-cid-image.eml", "remote-img-srcset.eml"] {
        let mut s = session(file, "browser-at-once");
        assert_eq!(s.keys("\rB").said(), "Opening the sealed copy in your browser · nothing remote loads", "{file}");
        assert_eq!(s.opened().len(), 1, "{file}");
        assert_eq!(policy(&s), "default-src 'none'; img-src data:; style-src 'unsafe-inline'", "{file}");
    }
}

#[test]
fn unsubscribe_opens_an_https_page_only_after_a_yes() {
    let mut s = session("unsubscribe-https-only.eml", "unsubscribe");
    let asked = s.keys("\r:unsubscribe\r").said();
    assert!(asked.starts_with("Unsubscribe at ") && asked.ends_with("?  y yes · n no"), "{asked}");
    assert!(s.keys("n").opened().is_empty());
    let opened = s.keys(":unsubscribe\ry").opened();
    assert!(matches!(opened.as_slice(), [url] if url.starts_with("https://")), "{opened:?}");
    for file in ["unsubscribe-http-only.eml", "unsubscribe-other-scheme.eml"] {
        let mut s = session(file, "unsubscribe");
        assert_eq!(s.keys("\r:unsubscribe\r").said(), "E :unsubscribe: this message names no way to", "{file}");
        assert!(s.keys("y").opened().is_empty(), "{file}");
    }
}

/// ADR-011: what an earlier run left to be shown goes after a day, and
/// what is newer stays for the browser that may still be reading it.
#[test]
fn what_was_shown_more_than_a_day_ago_is_removed_at_start() {
    let mut s = session("script-in-html.eml", "sweep");
    let copy = PathBuf::from(s.keys("\rBB").opened().remove(0));
    let (old, new) = (copy.parent().unwrap().to_path_buf(), PathBuf::from(&s.opened()[1]));
    // Dropped without its `Drop`, as a run that was killed leaves things.
    std::mem::forget(s.outside.take());
    let two_days = SystemTime::now() - Duration::from_secs(2 * 24 * 60 * 60);
    std::fs::File::open(&old).unwrap().set_modified(two_days).unwrap();
    let next = Outside::new(Box::new(Recorded(Rc::default())), s.root.join("downloads"), &s.root.join("cache"));
    assert!(!old.exists() && new.exists(), "old {} new {}", old.exists(), new.exists());
    drop(next);
    assert!(new.exists(), "a run removes only what it wrote itself");
}

/// ADR-008: a picture the message carries is drawn from its own bytes, in
/// the cells the reader gave its placeholder. The corpus's inline picture
/// is read, sized, placed and handed to a terminal's protocol.
#[test]
fn an_attached_picture_is_sized_placed_and_drawn_from_the_message_s_own_bytes() {
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use ratatui_image::picker::{Picker, ProtocolType};

    let mut s = session("inline-cid-image.eml", "picture");
    let [picture] = s.body.pictures.as_slice() else { panic!("{:?}", s.body.pictures) };
    let label = format!("chart.png  {}x{}", picture.width, picture.height);
    assert!(s.keys("\r").screen().iter().any(|row| row.contains(&label)), "{:#?}", s.screen());
    assert!(s.keys("a").screen().iter().any(|row| row.ends_with("image/png   shown above")));

    let area = Rect::new(0, 0, 160, 40);
    let mut buf = Buffer::empty(area);
    s.keys("q").app.render(area, &mut buf);
    let (id, placed) = s.app.pictures().unwrap();
    let [at] = placed else { panic!("{placed:?}") };
    let cells = Rect::new(at.col as u16, at.row as u16, at.width as u16, at.height as u16);
    assert!(cells.positions().all(|p| buf[p].symbol() == "░" || buf[p].symbol().is_ascii()), "the placeholder");
    let mut picker = Picker::halfblocks();
    picker.set_protocol_type(ProtocolType::Kitty);
    let bytes = |index| tuimex_content::attachment(&s.raw, index).ok().map(|file| file.data);
    crate::pictures::Painter::with(picker).paint(id, placed, bytes, &mut buf);
    assert!(cells.positions().all(|p| buf[p].symbol() != "░"), "the picture took the placeholder's cells");
    assert!(cells.positions().any(|p| buf[p].symbol().contains('\x1b')), "the protocol's sequence is in them");

    let none = session("attachment-mime-not-a-type.eml", "picture");
    assert!(none.body.pictures.is_empty(), "a file that is not a picture has no placeholder");
}

/// PLAN §10, 0.1.0: "an attachment opens in the system's program for its
/// type". The system tells the type by the file's name, so what the opener
/// is handed is the saved copy's path with the extension the sender gave
/// it, whatever else the safe-name rule took off (ADR-007).
#[test]
fn the_opener_is_handed_a_path_that_ends_in_the_files_own_name_and_extension() {
    let part = |kind: &str, name: &str| {
        format!("--edge\r\nContent-Type: {kind}\r\nContent-Disposition: attachment; filename=\"{name}\"\r\n\r\nx\r\n")
    };
    let office = "application/vnd.openxmlformats-officedocument";
    let sent = [
        ("application/pdf", "report.pdf", "report.pdf"),
        (&*format!("{office}.wordprocessingml.document"), "Notes for Ada.docx", "Notes for Ada.docx"),
        (&*format!("{office}.spreadsheetml.sheet"), "../../.config/sheet.xlsx", "sheet.xlsx"),
        ("image/png", "~/-picture.PNG", "picture.PNG"),
        ("application/octet-stream", "README", "README"),
        ("application/pdf", "invoice.pdf.", "invoice.pdf"),
    ];
    let parts: String = sent.iter().map(|(kind, name, _)| part(kind, name)).collect();
    let raw = format!(
        "From: Grace Hopper <grace@example.net>\r\nTo: ada@example.org\r\nSubject: files\r\nMIME-Version: 1.0\r\n\
         Content-Type: multipart/mixed; boundary=edge\r\n\r\n--edge\r\nContent-Type: text/plain\r\n\r\nFiles.\r\n\
         {parts}--edge--\r\n"
    )
    .into_bytes();
    let body = read(&raw).unwrap();
    let root = Dirs::from_env().unwrap().cache.join("handed");
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    let opened = Rc::new(RefCell::new(Vec::new()));
    let mut outside = Outside::new(Box::new(Recorded(opened.clone())), root.join("downloads"), &root.join("cache"));
    for (index, (_, _, name)) in sent.iter().enumerate() {
        let ask = Ask { serial: index as u64 + 1, action: Action::OpenAttachment(index), on: vec![id()] };
        assert_eq!(outside.carry_out(&ask, &body, Some(&raw)), None, "{name}");
        let handed = opened.borrow().last().cloned().unwrap();
        let path = Path::new(&handed);
        assert_eq!(path.file_name().and_then(|n| n.to_str()), Some(*name));
        assert!(path.is_absolute() && path.starts_with(root.join("cache/shown")) && path.is_file(), "{handed}");
    }
    assert_eq!(opened.borrow().len(), sent.len(), "one launch for each, and nothing else");
}

#[test]
fn a_saved_file_under_the_home_directory_is_said_from_a_tilde_and_any_other_whole() {
    let home = Path::new("/home/someone");
    assert_eq!(homely(Path::new("/home/someone/Downloads/a.pdf"), Some(home)), "~/Downloads/a.pdf");
    assert_eq!(homely(Path::new("/home/someone-else/a.pdf"), Some(home)), "/home/someone-else/a.pdf");
    assert_eq!(homely(Path::new("/srv/files/a.pdf"), None), "/srv/files/a.pdf");
}

/// What opens things for the calendar, under a scratch root, and what it
/// was handed to open.
fn outside(test: &str) -> (Outside, Rc<RefCell<Vec<String>>>, PathBuf) {
    let root = Dirs::from_env().unwrap().cache.join(test);
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    let opened = Rc::new(RefCell::new(Vec::new()));
    let outside = Outside::new(Box::new(Recorded(opened.clone())), root.join("downloads"), &root.join("cache"));
    (outside, opened, root)
}

/// An event that joins at its `links`, the first of them its meeting's.
fn event(links: &[&str], location: &str) -> tuimex_core::calendar::Event {
    tuimex_core::calendar::Event {
        uid: "planning@example.com".into(),
        sequence: 0,
        instead_of: None,
        summary: "Planning".into(),
        start: tuimex_core::calendar::Moment::Utc(1_795_687_200),
        end: None,
        location: Some(location.to_string()),
        description: None,
        organizer: None,
        guests: Vec::new(),
        cancelled: false,
        links: links.iter().map(|link| (*link).to_string()).collect(),
        repeat: None,
        excluded: Vec::new(),
        alarms: Vec::new(),
        more: Vec::new(),
    }
}

/// ADR-018, ADR-054: an event's link is a link. Whatever the interface
/// made of it, it is assessed again where it is opened, and a refusal
/// names the key that asked.
#[test]
fn an_event_s_link_is_assessed_again_where_it_opens_and_a_refusal_names_its_key() {
    let (mut outside, opened, _) = outside("event-link");
    let meeting = event(&["https://meet.example.com/abc-defg", "file:///etc/passwd"], "Room Fjord");
    assert_eq!(outside.event_link(1, Some(&meeting), 1), None);
    assert_eq!(*opened.borrow(), ["https://meet.example.com/abc-defg"]);
    let refused = |reason: &str| Some(Event::ActionFailed { serial: 2, reason: reason.into(), hint: String::new() });
    assert_eq!(outside.event_link(2, Some(&meeting), 2), refused("o 2: only web links are opened"));
    assert_eq!(outside.event_link(2, Some(&meeting), 3), refused("o 3: no such link"));
    assert_eq!(outside.event_link(2, None, 1), refused("o 1: the event is no longer in the calendar"));
    assert_eq!(opened.borrow().len(), 1, "one link was asked for and is the one thing opened");
}

/// ADR-055, ADR-011: `:export` writes one new file, owner-only, and never
/// over a file that is there.
#[test]
fn the_local_events_are_written_to_a_new_file_and_never_over_one() {
    let (mut outside, opened, root) = outside("export");
    let text = "BEGIN:VCALENDAR\r\nEND:VCALENDAR\r\n".to_string();
    let downloads = root.join("downloads");
    let saved = |path: &Path| Event::Saved { path: shown(path) };
    assert_eq!(outside.export(1, Path::new(""), Ok(Some(text.clone()))), saved(&downloads.join("tuimex-calendar.ics")));
    assert_eq!(std::fs::read_to_string(downloads.join("tuimex-calendar.ics")).unwrap(), text);
    let mode = std::fs::metadata(downloads.join("tuimex-calendar.ics")).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
    let again = outside.export(2, Path::new(""), Ok(Some(text.clone())));
    assert_eq!(again, saved(&downloads.join("tuimex-calendar-2.ics")), "beside the first, not over it");

    assert_eq!(outside.export(3, Path::new("mine.ics"), Ok(Some(text.clone()))), saved(&downloads.join("mine.ics")));
    let refused = |serial, reason: &str| Event::ActionFailed { serial, reason: reason.into(), hint: String::new() };
    let taken = outside.export(4, Path::new("mine.ics"), Ok(Some("other".into())));
    assert_eq!(taken, refused(4, ":export: mine.ics is there already"));
    assert_eq!(std::fs::read_to_string(downloads.join("mine.ics")).unwrap(), text);
    let none = outside.export(5, Path::new(""), Ok(None));
    assert_eq!(none, refused(5, ":export: tuimex holds no local event to write"));
    let unread = outside.export(6, Path::new(""), Err("the calendar could not be kept: disk".into()));
    assert_eq!(unread, refused(6, ":export: the calendar could not be kept: disk"));
    assert!(opened.borrow().is_empty(), "writing a file opens nothing");
}
