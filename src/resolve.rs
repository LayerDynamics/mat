//! SSRF defense, URL parsing, local-image path resolution, and the
//! pinned-address `ureq::Resolver`.
//!
//! `fetch_remote_image_to_temp` (in `image`) reaches out to an arbitrary
//! http(s) URL embedded in attacker-controlled markdown. Without
//! restrictions, that URL can target loopback, RFC1918 private ranges,
//! link-local (169.254/16 — includes the IMDS at 169.254.169.254), or
//! IPv6 ULA / link-local space. HTTP redirects can bounce from a public
//! host to an internal one after the initial validation. Defense:
//!
//!   - gated entry (opt-in `--allow-remote-images`);
//!   - pre-connect DNS resolution + IP classification;
//!   - a pinned-address `ureq::Resolver` so the actual connection targets
//!     exactly the validated addresses (DNS rebinding resistance);
//!   - `.redirects(0)` so the server cannot bounce us past the gate.

use std::cell::Cell;
use std::io;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, ToSocketAddrs};
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// IP classification
// ---------------------------------------------------------------------------

pub fn is_ip_forbidden_for_remote_fetch(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_ipv4_forbidden(v4),
        IpAddr::V6(v6) => is_ipv6_forbidden(v6),
    }
}

pub fn is_ipv4_forbidden(v4: Ipv4Addr) -> bool {
    if v4.is_loopback()
        || v4.is_private()
        || v4.is_link_local()
        || v4.is_multicast()
        || v4.is_broadcast()
        || v4.is_unspecified()
    {
        return true;
    }
    let o = v4.octets();
    if o[0] == 100 && (o[1] & 0b1100_0000) == 0b0100_0000 {
        return true;
    }
    if o[0] == 192 && o[1] == 0 && o[2] == 0 {
        return true;
    }
    if o[0] == 192 && o[1] == 0 && o[2] == 2 {
        return true;
    }
    if o[0] == 198 && (o[1] == 18 || o[1] == 19) {
        return true;
    }
    if o[0] == 198 && o[1] == 51 && o[2] == 100 {
        return true;
    }
    if o[0] == 203 && o[1] == 0 && o[2] == 113 {
        return true;
    }
    if (o[0] & 0xf0) == 0xf0 {
        return true;
    }
    false
}

pub fn is_ipv6_forbidden(v6: Ipv6Addr) -> bool {
    if v6.is_loopback() || v6.is_unspecified() || v6.is_multicast() {
        return true;
    }
    if let Some(v4) = v6.to_ipv4_mapped()
        && is_ipv4_forbidden(v4)
    {
        return true;
    }
    if let Some(v4) = v6.to_ipv4()
        && is_ipv4_forbidden(v4)
    {
        return true;
    }
    let seg = v6.segments();
    if (seg[0] & 0xfe00) == 0xfc00 {
        return true;
    }
    if (seg[0] & 0xffc0) == 0xfe80 {
        return true;
    }
    if seg[0] == 0x2001 && seg[1] == 0x0db8 {
        return true;
    }
    if seg[0] == 0x0100 && seg[1] == 0 && seg[2] == 0 && seg[3] == 0 {
        return true;
    }
    if seg[0] == 0x2001 && (seg[1] & 0xfe00) == 0 {
        return true;
    }
    false
}

// ---------------------------------------------------------------------------
// URL parsing
// ---------------------------------------------------------------------------

pub struct ParsedHttpUrl {
    pub host: String,
    pub port: u16,
}

pub fn parse_http_url(url: &str) -> Option<ParsedHttpUrl> {
    let (scheme, rest) = url.split_once("://")?;
    let default_port = match scheme {
        "http" => 80_u16,
        "https" => 443_u16,
        _ => return None,
    };
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..authority_end];
    let host_port = match authority.rsplit_once('@') {
        Some((_userinfo, hp)) => hp,
        None => authority,
    };
    if host_port.is_empty() {
        return None;
    }
    if let Some(after_bracket) = host_port.strip_prefix('[') {
        let end = after_bracket.find(']')?;
        let host = &after_bracket[..end];
        if host.is_empty() {
            return None;
        }
        let after = &after_bracket[end + 1..];
        let port = if let Some(p) = after.strip_prefix(':') {
            p.parse().ok()?
        } else if after.is_empty() {
            default_port
        } else {
            return None;
        };
        return Some(ParsedHttpUrl {
            host: host.to_string(),
            port,
        });
    }
    if let Some((h, p)) = host_port.rsplit_once(':') {
        if h.contains(':') || h.is_empty() {
            return None;
        }
        let port: u16 = p.parse().ok()?;
        Some(ParsedHttpUrl {
            host: h.to_string(),
            port,
        })
    } else {
        Some(ParsedHttpUrl {
            host: host_port.to_string(),
            port: default_port,
        })
    }
}

// ---------------------------------------------------------------------------
// Fetch policy + pinned resolver
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
pub struct ImageFetchPolicy {
    pub allow_loopback: bool,
    pub allow_private: bool,
}

impl ImageFetchPolicy {
    pub fn strict() -> Self {
        Self {
            allow_loopback: false,
            allow_private: false,
        }
    }

    pub fn permits(&self, ip: IpAddr) -> bool {
        if !is_ip_forbidden_for_remote_fetch(ip) {
            return true;
        }
        match ip {
            IpAddr::V4(v4) => {
                if self.allow_loopback && v4.is_loopback() {
                    return true;
                }
                if self.allow_private && (v4.is_private() || v4.is_link_local()) {
                    return true;
                }
                false
            }
            IpAddr::V6(v6) => {
                if self.allow_loopback && v6.is_loopback() {
                    return true;
                }
                if self.allow_private {
                    let seg = v6.segments();
                    let ula = (seg[0] & 0xfe00) == 0xfc00;
                    let ll = (seg[0] & 0xffc0) == 0xfe80;
                    if ula || ll {
                        return true;
                    }
                }
                false
            }
        }
    }
}

pub struct AllowListResolver {
    pub addrs: Vec<SocketAddr>,
}

impl ureq::Resolver for AllowListResolver {
    fn resolve(&self, _netloc: &str) -> io::Result<Vec<SocketAddr>> {
        Ok(self.addrs.clone())
    }
}

// ---------------------------------------------------------------------------
// Local image path validation (traversal defense)
// ---------------------------------------------------------------------------

pub fn resolve_local_image_path(
    url: &str,
    base_dir: &Path,
    allow_absolute: bool,
) -> Result<PathBuf, &'static str> {
    if url.is_empty() {
        return Err("empty path");
    }
    let requested = Path::new(url);
    let joined = if requested.is_absolute() {
        if !allow_absolute {
            return Err("absolute image paths disabled");
        }
        requested.to_path_buf()
    } else {
        base_dir.join(requested)
    };
    if !joined.exists() {
        return Err("not found");
    }
    let canon_target = joined.canonicalize().map_err(|_| "inaccessible")?;
    if allow_absolute {
        return Ok(canon_target);
    }
    let canon_base = base_dir.canonicalize().map_err(|_| "inaccessible base")?;
    if canon_target.starts_with(&canon_base) {
        Ok(canon_target)
    } else {
        Err("outside source directory")
    }
}

// ---------------------------------------------------------------------------
// SSRF host resolution + validation
// ---------------------------------------------------------------------------

/// SSRF guard: resolve `host` and test every returned IP through
/// `ImageFetchPolicy::permits`, which in turn consults the
/// `is_ip_forbidden_for_remote_fetch` classifier. Loopback is only accepted
/// under the test-only `AllowLoopbackGuard` RAII bypass so tiny_http-backed
/// tests keep working. Returns the validated IP addresses so the caller can
/// feed them to `AllowListResolver` (DNS-rebinding resistance — the socket
/// targets exactly the IPs we validated, not whatever DNS returns later).
pub fn resolve_and_validate_host(host: &str) -> Result<Vec<IpAddr>, String> {
    let policy = if ssrf_bypass_active() {
        ImageFetchPolicy {
            allow_loopback: true,
            allow_private: false,
        }
    } else {
        ImageFetchPolicy::strict()
    };

    if let Ok(ip) = host.parse::<IpAddr>() {
        if !policy.permits(ip) {
            return Err(format!("blocked IP: {ip}"));
        }
        return Ok(vec![ip]);
    }

    let probe = format!("{host}:0");
    // Call through ToSocketAddrs explicitly so the trait stays visible as
    // part of this function's contract (DNS resolution happens here, not
    // some opaque downstream crate).
    let addrs: Vec<SocketAddr> = match ToSocketAddrs::to_socket_addrs(&probe) {
        Ok(it) => it.collect(),
        Err(e) => return Err(format!("dns: {e}")),
    };
    if addrs.is_empty() {
        return Err("dns: no addresses".to_string());
    }
    let mut ips: Vec<IpAddr> = Vec::with_capacity(addrs.len());
    for sa in &addrs {
        let ip = sa.ip();
        if !policy.permits(ip) {
            return Err(format!("blocked IP via DNS: {ip}"));
        }
        ips.push(ip);
    }
    Ok(ips)
}

/// Resolve a Location header against a base URL. Supports absolute URLs and
/// root-relative / path-relative paths. Returns None if the target is neither
/// an http/https URL nor a resolvable relative reference.
pub fn resolve_redirect_target(base: &str, location: &str) -> Option<String> {
    if location.starts_with("http://") || location.starts_with("https://") {
        return Some(location.to_string());
    }
    // Scheme-relative "//host/path" → inherit base scheme.
    if let Some(rest) = location.strip_prefix("//") {
        let scheme = if base.starts_with("https://") {
            "https:"
        } else {
            "http:"
        };
        return Some(format!("{scheme}//{rest}"));
    }
    // Root-relative "/path?q" → base scheme + base host + location.
    let rest = base
        .strip_prefix("http://")
        .or_else(|| base.strip_prefix("https://"))?;
    let authority_end = rest.find('/').unwrap_or(rest.len());
    let scheme = if base.starts_with("https://") {
        "https://"
    } else {
        "http://"
    };
    let base_authority = &rest[..authority_end];
    if let Some(path) = location.strip_prefix('/') {
        return Some(format!("{scheme}{base_authority}/{path}"));
    }
    // Path-relative (no leading slash) — drop the last path segment of base
    // and append the new one. Rare in practice for image redirects, but
    // handled for completeness.
    let base_path = &rest[authority_end..];
    let parent = base_path.rsplit_once('/').map(|(h, _)| h).unwrap_or("");
    Some(format!("{scheme}{base_authority}{parent}/{location}"))
}

// ---------------------------------------------------------------------------
// Test-opt-in SSRF bypass
// ---------------------------------------------------------------------------
//
// `AllowLoopbackGuard` + `ssrf_bypass_active` are *always* compiled (not
// #[cfg(test)]-gated) so integration tests — which link against the
// library's *non-test* build — can opt into loopback fetches for
// tiny_http-backed fixtures. The binary never constructs a guard, so the
// bypass is never active in production code.

thread_local! {
    static ALLOW_LOOPBACK: Cell<bool> = const { Cell::new(false) };
}

pub struct AllowLoopbackGuard {
    prev: bool,
}

impl AllowLoopbackGuard {
    pub fn new() -> Self {
        let prev = ALLOW_LOOPBACK.with(|c| c.replace(true));
        Self { prev }
    }
}

impl Default for AllowLoopbackGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for AllowLoopbackGuard {
    fn drop(&mut self) {
        let prev = self.prev;
        ALLOW_LOOPBACK.with(|c| c.set(prev));
    }
}

pub fn ssrf_bypass_active() -> bool {
    ALLOW_LOOPBACK.with(|c| c.get())
}
