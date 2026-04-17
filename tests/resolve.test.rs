//! Resolver tests — SSRF IP classification, URL authority parsing, local
//! image path traversal defense, and the redirect-location resolver.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use mat::image::fetch_remote_image_to_temp;
use mat::resolve::{
    ImageFetchPolicy, is_ip_forbidden_for_remote_fetch, parse_http_url, resolve_local_image_path,
    resolve_redirect_target,
};

// =====================================================================
// IP classification
// =====================================================================

#[test]
fn internal_addresses_are_forbidden() {
    for ip in [
        "127.0.0.1",
        "169.254.169.254",
        "10.0.0.1",
        "172.16.0.1",
        "192.168.1.1",
        "::1",
        "fc00::1",
        "fe80::1",
    ] {
        let parsed: IpAddr = ip.parse().unwrap();
        assert!(
            is_ip_forbidden_for_remote_fetch(parsed),
            "IP {ip} must be forbidden"
        );
    }
}

#[test]
fn public_address_is_allowed() {
    let ip: IpAddr = "1.1.1.1".parse().unwrap();
    assert!(!is_ip_forbidden_for_remote_fetch(ip));
}

#[test]
fn strict_policy_denies_all_internal_classes() {
    let strict = ImageFetchPolicy::strict();
    let v4: Ipv4Addr = "192.168.42.7".parse().unwrap();
    assert!(!strict.permits(IpAddr::V4(v4)), "RFC1918 must be forbidden");
    let v6: Ipv6Addr = "fc00::1".parse().unwrap();
    assert!(!strict.permits(IpAddr::V6(v6)), "ULA must be forbidden");
    let sa: SocketAddr = "8.8.8.8:80".parse().unwrap();
    assert!(strict.permits(sa.ip()), "public IP must be allowed");
}

#[test]
fn loopback_policy_permits_loopback_only_when_enabled() {
    let relaxed = ImageFetchPolicy {
        allow_loopback: true,
        allow_private: false,
    };
    let v4: Ipv4Addr = "127.0.0.1".parse().unwrap();
    assert!(relaxed.permits(IpAddr::V4(v4)));
    let v4: Ipv4Addr = "10.0.0.1".parse().unwrap();
    assert!(!relaxed.permits(IpAddr::V4(v4)));
}

// =====================================================================
// parse_http_url authority forms
// =====================================================================

#[test]
fn extract_host_handles_authority_variants() {
    fn host(url: &str) -> Option<String> {
        parse_http_url(url).map(|p| p.host)
    }
    assert_eq!(host("http://example.com/").as_deref(), Some("example.com"));
    assert_eq!(
        host("https://example.com:443/x?y=1").as_deref(),
        Some("example.com")
    );
    assert_eq!(
        host("http://user:pass@host.tld/x").as_deref(),
        Some("host.tld")
    );
    assert_eq!(host("http://[::1]/x").as_deref(), Some("::1"));
    assert_eq!(
        host("http://[2001:db8::1]:80/x").as_deref(),
        Some("2001:db8::1")
    );
    // Non-http(s) scheme and non-URL input both return None.
    assert_eq!(host("ftp://example.com/"), None);
    assert_eq!(host("not a url"), None);
}

#[test]
fn parse_http_url_supplies_default_ports() {
    let h = parse_http_url("http://host/").unwrap();
    assert_eq!(h.host, "host");
    assert_eq!(h.port, 80);
    let s = parse_http_url("https://host/").unwrap();
    assert_eq!(s.port, 443);
    let p = parse_http_url("http://host:8080/").unwrap();
    assert_eq!(p.port, 8080);
}

// =====================================================================
// resolve_redirect_target
// =====================================================================

#[test]
fn redirect_target_absolute_passes_through() {
    let next = resolve_redirect_target("http://a/1", "https://b/2").unwrap();
    assert_eq!(next, "https://b/2");
}

#[test]
fn redirect_target_root_relative_inherits_scheme_and_host() {
    let next = resolve_redirect_target("https://a.example/old", "/new/path").unwrap();
    assert_eq!(next, "https://a.example/new/path");
}

#[test]
fn redirect_target_scheme_relative_inherits_base_scheme() {
    let next = resolve_redirect_target("https://a.example/x", "//c.example/y").unwrap();
    assert_eq!(next, "https://c.example/y");
}

#[test]
fn redirect_target_path_relative_joins_to_parent() {
    let next = resolve_redirect_target("http://a.example/dir/page.html", "other.html").unwrap();
    assert_eq!(next, "http://a.example/dir/other.html");
}

// =====================================================================
// resolve_local_image_path — path traversal guard
// =====================================================================

#[test]
fn absolute_path_rejected_by_default() {
    let base = std::env::current_dir().unwrap();
    let err = resolve_local_image_path("/etc/passwd", &base, false).unwrap_err();
    assert_eq!(err, "absolute image paths disabled");
}

#[test]
fn parent_traversal_rejected() {
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path();
    let outside = tmp.path().parent().unwrap().join("mat_trav_outside");
    std::fs::write(&outside, b"x").unwrap();
    let rel = format!("../{}", outside.file_name().unwrap().to_string_lossy());
    let err = resolve_local_image_path(&rel, base, false).unwrap_err();
    assert_eq!(err, "outside source directory");
    let _ = std::fs::remove_file(&outside);
}

#[test]
fn sibling_file_allowed() {
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path();
    let target = base.join("ok.png");
    std::fs::write(&target, b"payload").unwrap();
    let got = resolve_local_image_path("ok.png", base, false).unwrap();
    assert_eq!(got, target.canonicalize().unwrap());
}

#[test]
fn absolute_allowed_under_optin() {
    let candidate = std::path::PathBuf::from("/bin/sh");
    if !candidate.exists() {
        return;
    }
    let base = std::env::current_dir().unwrap();
    let got = resolve_local_image_path("/bin/sh", &base, true).unwrap();
    assert_eq!(got, candidate.canonicalize().unwrap());
}

#[test]
fn empty_path_rejected() {
    let base = std::env::current_dir().unwrap();
    assert_eq!(
        resolve_local_image_path("", &base, false).unwrap_err(),
        "empty path"
    );
}

#[test]
fn nonexistent_file_rejected() {
    let tmp = tempfile::tempdir().unwrap();
    let err = resolve_local_image_path("ghost.png", tmp.path(), false).unwrap_err();
    assert_eq!(err, "not found");
}

// =====================================================================
// End-to-end SSRF refusal via fetch_remote_image_to_temp
// =====================================================================

#[test]
fn fetch_rejects_ssrf_loopback() {
    // No AllowLoopbackGuard here — we want the guard active. The
    // policy-backed path returns "blocked IP: 127.0.0.1" (IP literal) or
    // "blocked IP via DNS: ..." (DNS form); both contain the substring
    // "blocked" so we assert on that.
    let err = fetch_remote_image_to_temp("http://127.0.0.1:9/x.png").unwrap_err();
    assert!(err.contains("blocked"), "got: {err}");
    let err = fetch_remote_image_to_temp("http://[::1]:9/x.png").unwrap_err();
    assert!(err.contains("blocked"), "got: {err}");
}

#[test]
fn fetch_rejects_ssrf_rfc1918() {
    for host in ["10.0.0.1", "172.16.5.5", "192.168.1.1"] {
        let url = format!("http://{host}/x.png");
        let err = fetch_remote_image_to_temp(&url).unwrap_err();
        assert!(
            err.contains("blocked"),
            "{host} must be blocked; got: {err}"
        );
    }
}
