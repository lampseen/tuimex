//! Tracking services, recognised by where their images are served from.

/// Domain, and the service a reader would know it by. A host matches a
/// domain when it is that domain or under it.
const SERVICES: [(&str, &str); 24] = [
    ("list-manage.com", "Mailchimp"),
    ("mandrillapp.com", "Mailchimp"),
    ("sendgrid.net", "SendGrid"),
    ("mailgun.org", "Mailgun"),
    ("awstrack.me", "Amazon SES"),
    ("hubspotemail.net", "HubSpot"),
    ("hubspot.com", "HubSpot"),
    ("mktotracking.com", "Marketo"),
    ("exct.net", "Salesforce"),
    ("exacttarget.com", "Salesforce"),
    ("createsend.com", "Campaign Monitor"),
    ("rs6.net", "Constant Contact"),
    ("klaviyomail.com", "Klaviyo"),
    ("customeriomail.com", "Customer.io"),
    ("pstmrk.it", "Postmark"),
    ("mjt.lu", "Mailjet"),
    ("sendibt2.com", "Brevo"),
    ("convertkit-mail.com", "ConvertKit"),
    ("intercom-mail.com", "Intercom"),
    ("emltrk.com", "Litmus"),
    ("mailtrack.io", "Mailtrack"),
    ("yesware.com", "Yesware"),
    ("mixmax.com", "Mixmax"),
    ("google-analytics.com", "Google Analytics"),
];

pub(crate) fn service(host: &str) -> Option<&'static str> {
    SERVICES
        .iter()
        .find(|(domain, _)| host.strip_suffix(domain).is_some_and(|sub| sub.is_empty() || sub.ends_with('.')))
        .map(|(_, name)| *name)
}
