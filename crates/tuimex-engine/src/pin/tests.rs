use std::fs;
use std::os::unix::fs::PermissionsExt;

use super::{PinError, check, fingerprint, load, save};
use crate::tests::scratch;

/// Any bytes will do: nothing here reads inside a certificate.
const OLD: &[u8] = b"a synthetic certificate, long enough to be wrapped over more than one line of sixty-four";
const NEW: &[u8] = b"the certificate after the server made itself a new one";

#[test]
fn a_fingerprint_is_sha_256_as_openssl_prints_it() {
    let abc = "BA:78:16:BF:8F:01:CF:EA:41:41:40:DE:5D:AE:22:23:B0:03:61:A3:96:17:7A:9C:B4:10:FF:61:F2:00:15:AD";
    assert_eq!(fingerprint(b"abc"), abc);
}

#[test]
fn a_saved_certificate_is_an_owner_only_pem_file_named_for_its_account_and_loads_back() {
    let config = scratch("pin-save").join("config");
    save(&config, "bridge", OLD).unwrap();
    let path = config.join("bridge.pem");
    let pem = fs::read_to_string(&path).unwrap();
    assert!(pem.starts_with("-----BEGIN CERTIFICATE-----\n") && pem.ends_with("\n-----END CERTIFICATE-----\n"));
    assert!(pem.lines().all(|line| line.len() <= 64), "{pem}");
    assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
    assert_eq!(fs::metadata(&config).unwrap().permissions().mode() & 0o777, 0o700);
    assert_eq!(load(&config, "bridge").unwrap().as_deref(), Some(OLD));
    assert_eq!(load(&config, "other").unwrap(), None);
}

#[test]
fn accepting_a_renewed_certificate_replaces_the_old_one() {
    let config = scratch("pin-renew").join("config");
    save(&config, "bridge", OLD).unwrap();
    save(&config, "bridge", NEW).unwrap();
    assert_eq!(load(&config, "bridge").unwrap().as_deref(), Some(NEW));
    assert_eq!(fs::read_dir(&config).unwrap().count(), 1);
}

#[test]
fn the_pinned_certificate_passes_and_a_changed_one_names_both_fingerprints() {
    check("bridge", OLD, OLD).unwrap();
    let error = check("bridge", OLD, NEW).unwrap_err();
    assert!(matches!(error, PinError::Changed { .. }));
    let said = error.to_string();
    assert!(said.contains("bridge") && said.contains("tuimex setup"), "{said}");
    assert!(said.contains(&format!("pinned:    {}", fingerprint(OLD))), "{said}");
    assert!(said.contains(&format!("presented: {}", fingerprint(NEW))), "{said}");
}

#[test]
fn an_account_name_cannot_place_the_file_outside_the_config_directory() {
    let config = scratch("pin-name").join("config");
    for name in ["", "../escape", ".hidden", "a/b"] {
        assert!(matches!(save(&config, name, OLD), Err(PinError::Name(_))), "{name:?}");
        assert!(matches!(load(&config, name), Err(PinError::Name(_))), "{name:?}");
    }
    assert!(!config.exists());
}

#[test]
fn a_file_that_holds_no_certificate_is_an_error_and_not_an_absent_pin() {
    let config = scratch("pin-garbage").join("config");
    save(&config, "bridge", OLD).unwrap();
    fs::write(config.join("bridge.pem"), "not a certificate").unwrap();
    assert!(matches!(load(&config, "bridge"), Err(PinError::NotACertificate { .. })));
}
