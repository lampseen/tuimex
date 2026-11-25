use super::*;

const ANSWER: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav" xmlns:CS="http://calendarserver.org/ns/">
 <D:response>
  <D:href>/calendar/dav/robin%40example.com/events/</D:href>
  <D:propstat>
   <D:status>HTTP/1.1 200 OK</D:status>
   <D:prop>
    <D:displayname> Robin's
      calendar </D:displayname>
    <CS:getctag>63889999</CS:getctag>
    <D:resourcetype><D:collection/><C:calendar/></D:resourcetype>
   </D:prop>
  </D:propstat>
  <D:propstat>
   <D:status>HTTP/1.1 404 Not Found</D:status>
   <D:prop><D:getetag>"never"</D:getetag></D:prop>
  </D:propstat>
 </D:response>
 <D:response>
  <D:href>/calendar/dav/robin%40example.com/events/one.ics</D:href>
  <D:propstat>
   <D:prop>
    <D:getetag>"63889999-1"</D:getetag>
    <C:calendar-data>BEGIN:VCALENDAR
BEGIN:VEVENT
UID:one@example.com
DTSTART:20261126T100000Z
SUMMARY:Review &amp; plan
END:VEVENT
END:VCALENDAR
</C:calendar-data>
   </D:prop>
   <D:status>HTTP/1.1 200 OK</D:status>
  </D:propstat>
 </D:response>
 <D:response>
  <D:href>/calendar/dav/robin%40example.com/events/gone.ics</D:href>
  <D:status>HTTP/1.1 404 Not Found</D:status>
 </D:response>
 <D:response><D:status>HTTP/1.1 200 OK</D:status></D:response>
</D:multistatus>"#;

#[test]
fn each_response_gives_where_it_is_and_what_the_server_answered_with_success() {
    let items = multistatus(ANSWER).unwrap();
    assert_eq!(items.len(), 3, "a response with no address is not an item");
    let collection = &items[0];
    assert_eq!(collection.href, "/calendar/dav/robin%40example.com/events/");
    assert_eq!(collection.name.as_deref(), Some("Robin's calendar"));
    assert_eq!(collection.ctag.as_deref(), Some("63889999"));
    assert!(collection.is_calendar);
    assert_eq!(collection.etag, None, "a property the server refused is absent");
    let one = &items[1];
    assert_eq!(one.etag.as_deref(), Some("\"63889999-1\""));
    assert!(!one.is_calendar && !one.gone);
    let events = crate::calendar(one.calendar.as_deref().unwrap()).events;
    assert_eq!((events[0].uid.as_str(), events[0].summary.as_str()), ("one@example.com", "Review & plan"));
    assert!(items[2].gone);
    assert_eq!((&items[2].etag, &items[2].calendar), (&None, &None));
}

#[test]
fn a_name_in_another_namespace_is_not_the_property() {
    let xml = r#"<multistatus xmlns="DAV:" xmlns:X="https://example.org/ns"><response><href>/a</href>
        <propstat><prop><X:getetag>"x"</X:getetag><X:calendar-data>BEGIN:VCALENDAR</X:calendar-data><getctag>1</getctag></prop></propstat>
        </response></multistatus>"#;
    assert_eq!(multistatus(xml).unwrap(), vec![DavItem { href: "/a".into(), ..DavItem::default() }]);
}

#[test]
fn what_is_not_a_multistatus_is_refused_and_says_why() {
    for (xml, why) in [
        ("", "the calendar server's answer could not be read"),
        ("<html><body>Sign in</body></html>", "it is not a multistatus"),
        ("<multistatus/>", "it is not a multistatus"),
        ("<D:multistatus xmlns:D=\"DAV:\"><D:response>", "could not be read"),
    ] {
        let error = multistatus(xml).unwrap_err().to_string();
        assert!(error.contains(why), "{xml}: {error}");
    }
    assert_eq!(multistatus("<multistatus xmlns=\"DAV:\"/>").unwrap(), Vec::new());
}

#[test]
fn an_answer_that_declares_entities_is_refused_whole() {
    // The classic expanding entity, and one that names a file.
    let bomb = r#"<?xml version="1.0"?><!DOCTYPE m [<!ENTITY a "aaaaaaaaaa"><!ENTITY b "&a;&a;&a;&a;&a;&a;&a;&a;">]>
        <multistatus xmlns="DAV:"><response><href>&b;</href></response></multistatus>"#;
    let file = r#"<?xml version="1.0"?><!DOCTYPE m [<!ENTITY x SYSTEM "file:///etc/passwd">]>
        <multistatus xmlns="DAV:"><response><href>&x;</href></response></multistatus>"#;
    for xml in [bomb, file] {
        assert!(multistatus(xml).is_err(), "{xml}");
    }
}

#[test]
fn hostile_text_in_an_answer_is_refused_or_cleaned() {
    // XML has no place for a control character, so the reader refuses the
    // answer; a character that reverses the text's direction is legal XML
    // and is taken out of what is shown.
    let with = |name: &str| {
        format!(
            "<multistatus xmlns=\"DAV:\"><response><href> /a\u{202e}b </href><propstat><prop><displayname>{name}</displayname></prop></propstat></response></multistatus>"
        )
    };
    assert!(multistatus(&with("\u{1b}]0;x\u{7}Work")).is_err());
    let items = multistatus(&with("W\u{202e}or\u{2066}k &lt;b&gt;")).unwrap();
    assert_eq!((items[0].href.as_str(), items[0].name.as_deref()), ("/ab", Some("Work <b>")));
}
