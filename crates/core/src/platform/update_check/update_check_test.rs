use std::io::{Read, Write};
use std::net::TcpListener;

use super::*;

/// Serves one canned HTTP response on a loopback port and returns the URL to request.
fn serve_once(status: &str, body: &str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/releases/latest", listener.local_addr().unwrap());
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut buffer = [0u8; 2048];
            let _ = stream.read(&mut buffer);
            let _ = stream.write_all(response.as_bytes());
        }
    });
    url
}

const RELEASE: &str = r#"{
    "tag_name": "v9.8.7",
    "html_url": "https://example.com/release/v9.8.7",
    "assets": [{"name": "oca-x86_64-unknown-linux-gnu-appimage.tar.gz"}, {"name": "notes.txt"}]
}"#;

#[test]
fn a_published_release_is_read_with_its_version_and_page() {
    let release = fetch_release_from(&serve_once("200 OK", RELEASE)).unwrap();
    assert_eq!(release.version, "9.8.7");
    assert_eq!(release.html_url, "https://example.com/release/v9.8.7");
    // Auto-update needs the exact AppImage archive for this machine's target, which a canned
    // release for another triple does not carry.
    assert!(
        !release.auto_update_available || cfg!(all(target_os = "linux", target_arch = "x86_64"))
    );
}

#[test]
fn a_release_without_assets_is_still_announced() {
    let body = r#"{"tag_name": "1.2.3", "html_url": "https://example.com/r"}"#;
    let release = fetch_release_from(&serve_once("200 OK", body)).unwrap();
    assert_eq!(release.version, "1.2.3");
    assert!(!release.auto_update_available);
}

#[test]
fn an_unreadable_answer_is_a_parse_error() {
    let error = fetch_release_from(&serve_once("200 OK", "not json")).unwrap_err();
    assert!(matches!(error, UpdateCheckError::Parse(_)));
    assert!(error.to_string().contains("failed to parse"));
}

#[test]
fn an_error_status_or_an_unreachable_server_is_a_request_error() {
    let error = fetch_release_from(&serve_once("404 Not Found", "{}")).unwrap_err();
    assert!(matches!(error, UpdateCheckError::Request(_)));
    assert!(error.to_string().contains("request failed"));

    let closed = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/", closed.local_addr().unwrap());
    drop(closed);
    assert!(matches!(
        fetch_release_from(&url),
        Err(UpdateCheckError::Request(_))
    ));
}

#[test]
fn versions_are_normalized_before_they_reach_a_tag_url() {
    assert_eq!(
        normalize_update_version(" v1.2.3-beta ").unwrap(),
        "1.2.3-beta"
    );
    assert_eq!(normalize_update_version("2.0").unwrap(), "2.0");
    for bad in ["", "   ", "v", "1.2/../3", "1 2", "v1.2.3?x=1"] {
        assert_eq!(
            normalize_update_version(bad),
            Err(ApplyUpdateError::InvalidVersion),
            "{bad:?}"
        );
    }
}

fn ok_install(plan: &InstallPlan<'_>) -> Result<ApplyUpdateOutcome, ApplyUpdateError> {
    Ok(ApplyUpdateOutcome::Updated {
        version: format!("{}|{}|{}", plan.tag, plan.target, plan.expected_asset),
    })
}

#[test]
fn a_supported_appimage_update_is_handed_the_exact_asset_to_install() {
    let outcome = apply_update_with(
        " v3.1.4 ",
        true,
        UpdatePackage::LinuxAppImage,
        "x86_64-unknown-linux-gnu",
        ok_install,
    )
    .unwrap();
    assert_eq!(
        outcome,
        ApplyUpdateOutcome::Updated {
            version: "v3.1.4|x86_64-unknown-linux-gnu|oca-x86_64-unknown-linux-gnu-appimage.tar.gz"
                .into()
        }
    );
}

#[test]
fn an_update_is_refused_before_installing_when_anything_is_off() {
    let never = |_: &InstallPlan<'_>| -> Result<ApplyUpdateOutcome, ApplyUpdateError> {
        panic!("the installer must not run")
    };
    let linux = "x86_64-unknown-linux-gnu";
    assert_eq!(
        apply_update_with("1.0.0", false, UpdatePackage::LinuxAppImage, linux, never),
        Err(ApplyUpdateError::UnsupportedPlatform)
    );
    assert_eq!(
        apply_update_with("../x", true, UpdatePackage::LinuxAppImage, linux, never),
        Err(ApplyUpdateError::InvalidVersion)
    );
    assert_eq!(
        apply_update_with("1.0.0", true, UpdatePackage::NativeBinary, linux, never),
        Err(ApplyUpdateError::UnsupportedPlatform)
    );
    assert_eq!(
        apply_update_with(
            "1.0.0",
            true,
            UpdatePackage::LinuxAppImage,
            "aarch64-apple-darwin",
            never
        ),
        Err(ApplyUpdateError::UnsupportedPlatform)
    );
}

#[test]
fn an_installer_failure_is_passed_through() {
    let result = apply_update_with(
        "1.0.0",
        true,
        UpdatePackage::LinuxAppImage,
        "x86_64-unknown-linux-gnu",
        |_| {
            Err(ApplyUpdateError::MissingAsset {
                expected: "x".into(),
            })
        },
    );
    assert_eq!(
        result,
        Err(ApplyUpdateError::MissingAsset {
            expected: "x".into()
        })
    );
}

#[test]
fn the_real_update_entry_point_declines_on_a_machine_that_cannot_replace_itself() {
    if !auto_update_supported() {
        assert_eq!(
            apply_update("1.0.0"),
            Err(ApplyUpdateError::UnsupportedPlatform)
        );
    }
}
