use super::Warning::{BareIpAddress, CarriesCredentials, MixedScripts, TextNamesAnotherHost};
use super::*;

#[test]
fn a_url_is_read_as_a_browser_reads_it() {
    // (as written, scheme, host, credentials)
    let table = [
        ("https://example.com/a", "https", "example.com", false),
        (" https://example.com/a", "https", "example.com", false),
        ("HTTPS://EXAMPLE.COM/a", "https", "example.com", false),
        ("ht\ntps://exa\tmple.com", "https", "example.com", false),
        ("https:example.com", "https", "example.com", false),
        ("https:\\\\example.com\\a", "https", "example.com", false),
        ("https://example.com:8443/a", "https", "example.com", false),
        ("https://example.com?x#y", "https", "example.com", false),
        ("https://user:pw@example.com/", "https", "example.com", true),
        ("https://example.com@example.net/", "https", "example.net", true),
        ("https://[2001:db8::1]:443/", "https", "[2001:db8::1]", false),
        ("//example.com/pixel.gif", "", "example.com", false),
        ("/etc/hostname", "", "", false),
        ("pixel.gif", "", "", false),
        ("#top", "", "", false),
        ("cid:logo@example.com", "cid", "", false),
        ("data:image/png;base64,AAAA", "data", "", false),
        ("file:///etc/passwd", "file", "", false),
        ("file://server.example.com/share", "file", "server.example.com", false),
        ("ssh://example.com/", "ssh", "example.com", false),
        ("javascript:alert(1)", "javascript", "", false),
        ("mailto:ada@example.com?subject=Hi", "mailto", "example.com", false),
        ("mailto:nobody", "mailto", "", false),
    ];
    for (raw, scheme, host, credentials) in table {
        let url = Url::parse(raw);
        assert_eq!((url.scheme.as_str(), url.host.as_str(), url.credentials), (scheme, host, credentials), "{raw:?}");
    }
}

#[test]
fn only_what_a_page_would_fetch_is_remote() {
    for (raw, remote) in [
        ("https://example.com/a.png", true),
        ("http://example.com/a.png", true),
        ("//example.com/a.png", true),
        ("cid:a@example.com", false),
        ("data:image/png;base64,AA", false),
        ("/etc/hostname", false),
        ("mailto:ada@example.com", false),
    ] {
        assert_eq!(Url::parse(raw).is_remote(), remote, "{raw}");
    }
}

#[test]
fn assess_warns_and_blocks_by_the_table() {
    // (url, shown, warnings, blocked)
    let table: [(&str, &str, &[Warning], bool); 26] = [
        ("https://example.com/", "Read more", &[], false),
        ("https://example.com/", "https://example.com/", &[], false),
        ("https://www.example.com/", "example.com", &[], false),
        ("https://example.com/", "www.example.com", &[], false),
        ("https://example.net/login", "https://example.com/login", &[TextNamesAnotherHost], false),
        ("https://example.net/login", "Sign in at example.com today", &[TextNamesAnotherHost], false),
        ("https://example.com.example.net/", "example.com", &[TextNamesAnotherHost], false),
        ("https://notexample.com/", "example.com", &[TextNamesAnotherHost], false),
        ("mailto:mallory@example.net", "ada@example.com", &[TextNamesAnotherHost], false),
        ("mailto:ada@example.com", "ada@example.com", &[], false),
        ("https://example.com@example.net/", "Read more", &[CarriesCredentials], false),
        ("http://192.0.2.7/", "Read more", &[BareIpAddress], false),
        ("http://3221225991/", "Read more", &[BareIpAddress], false),
        ("http://0xc0.0.2.7/", "Read more", &[BareIpAddress], false),
        ("http://[2001:db8::1]/", "Read more", &[BareIpAddress], false),
        ("https://p\u{430}ypal.example.com/", "Read more", &[MixedScripts], false),
        ("https://xn--pypal-4ve.example.com/", "Read more", &[MixedScripts], false),
        ("https://xn--mnchen-3ya.example.com/", "Read more", &[], false),
        ("https://xn--!!.example.com/", "Read more", &[MixedScripts], false),
        ("https://\u{43f}\u{440}\u{438}\u{43c}\u{435}\u{440}.example.com/", "Read more", &[], false),
        ("ssh://example.com/", "Read more", &[], true),
        ("file:///etc/passwd", "Read more", &[], true),
        ("javascript:alert(1)", "Read more", &[], true),
        ("/etc/hostname", "Read more", &[], true),
        ("https:///", "Read more", &[], true),
        ("mailto:nobody", "Read more", &[], true),
    ];
    for (url, shown, warnings, blocked) in table {
        let got = assess(url, shown);
        assert_eq!((got.warnings.as_slice(), got.blocked), (warnings, blocked), "{url} shown as {shown:?}");
    }
}

#[test]
fn assess_reports_the_real_host_whatever_the_text_says() {
    let got = assess("https://example.net/login", "https://example.com/login");
    assert_eq!(got.host, "example.net");
}

#[test]
fn click_tracking_parameters_are_removed_and_nothing_else() {
    for (url, want) in [
        ("https://example.com/a?utm_source=x&id=7&fbclid=abc#top", "https://example.com/a?id=7#top"),
        ("https://example.com/a?utm_source=x&UTM_Medium=y", "https://example.com/a"),
        ("https://example.com/a?id=7", "https://example.com/a?id=7"),
        ("https://example.com/a#utm_source=x", "https://example.com/a#utm_source=x"),
        ("https://example.com/a", "https://example.com/a"),
    ] {
        assert_eq!(assess(url, "").url, want);
    }
    assert_eq!(assess("ssh://example.com/?utm_source=x", "").url, "ssh://example.com/?utm_source=x");
}

#[test]
fn punycode_decodes_the_rfc_samples() {
    assert_eq!(punycode("mnchen-3ya").as_deref(), Some("m\u{fc}nchen"));
    assert_eq!(punycode("pypal-4ve").as_deref(), Some("p\u{430}ypal"));
    assert_eq!(punycode("e1afmkfd").as_deref(), Some("\u{43f}\u{440}\u{438}\u{43c}\u{435}\u{440}"));
    assert_eq!(punycode("99999999999999").as_deref(), None);
}
