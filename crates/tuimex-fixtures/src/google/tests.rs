use super::FakeGoogle;

#[test]
fn consent_is_refused_for_a_request_google_would_not_take() {
    let google = FakeGoogle::start("client", "secret").unwrap();
    let good = "client_id=client&redirect_uri=http://127.0.0.1:4000&response_type=code&scope=https://mail.google.com/\
                &code_challenge_method=S256&code_challenge=0123456789012345678901234567890123456789012&state=0123456789abcdef\
                &access_type=offline";
    let back = google.authorize(&format!("{}?{good}", google.authorize_url())).unwrap();
    assert!(back.starts_with("http://127.0.0.1:4000/?state=0123456789abcdef&code=4%2Fsynthetic-code-1"), "{back}");
    for (from, to) in [
        ("client_id=client", "client_id=another"),
        ("http://127.0.0.1:4000", "http://example.com"),
        ("S256", "plain"),
        ("access_type=offline", "access_type=online"),
    ] {
        let url = format!("{}?{}", google.authorize_url(), good.replace(from, to));
        assert!(google.authorize(&url).is_err(), "{to} was accepted");
    }
}
