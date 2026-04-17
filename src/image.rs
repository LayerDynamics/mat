//! Remote-image download + extension sniffing.
//!
//! `fetch_remote_image_to_temp` runs behind the SSRF guard in `resolve`.
//! It performs manual redirect following with every hop re-validated, a
//! pinned-address `ureq::Resolver`, a byte cap, a timeout, and content-type
//! checks. The renderer's `RenderState::render_image` (in `renderer`) is
//! the sole caller.

use std::io::{self, Read, Write};
use std::net::SocketAddr;
use std::time::Duration;

use tempfile::NamedTempFile;

use crate::resolve::{AllowListResolver, parse_http_url, resolve_and_validate_host, resolve_redirect_target};

/// Hard cap on bytes downloaded for a remote image (16 MiB). Prevents a
/// hostile or accidental endpoint from streaming forever.
pub const MAX_REMOTE_IMAGE_BYTES: u64 = 16 * 1024 * 1024;
/// Default ceiling on how many viewport-heights a single image may occupy
/// vertically. 2.5 preserves natural aspect ratio for typical landscape-
/// oriented screenshots at full terminal width without compressing their
/// text into unreadable pixel mush — the previous 0.6 default clamped a
/// 1.5:1 screenshot down to ~30% of terminal width because it couldn't
/// grow taller than 60% of the viewport. Users with text-only documents
/// or portrait-oriented images can override via `MAT_IMAGE_MAX_HEIGHT_SCALE`.
pub const IMAGE_MAX_VIEWPORT_FRACTION: f64 = 2.5;
/// Name of the environment variable that overrides
/// [`IMAGE_MAX_VIEWPORT_FRACTION`] at runtime. Accepts any positive decimal;
/// non-numeric / non-positive values are silently ignored.
pub const IMAGE_MAX_HEIGHT_SCALE_ENV: &str = "MAT_IMAGE_MAX_HEIGHT_SCALE";

/// Resolve the effective per-image vertical scale cap. Reads
/// `MAT_IMAGE_MAX_HEIGHT_SCALE` first, falls back to
/// [`IMAGE_MAX_VIEWPORT_FRACTION`]. Callers apply this against the current
/// viewport row count to get the row ceiling for a single rendered image.
pub fn image_max_height_scale() -> f64 {
    if let Ok(raw) = std::env::var(IMAGE_MAX_HEIGHT_SCALE_ENV)
        && let Ok(n) = raw.trim().parse::<f64>()
        && n.is_finite()
        && n > 0.0
    {
        return n;
    }
    IMAGE_MAX_VIEWPORT_FRACTION
}

/// Download a remote image (http/https) into a freshly-created NamedTempFile
/// and return that handle. The temp file is deleted when the handle drops.
pub fn fetch_remote_image_to_temp(url: &str) -> Result<NamedTempFile, String> {
    // SSRF guard: resolve and validate the host before issuing any request.
    // Without this, a hostile markdown file could trick `mat` into probing
    // 127.0.0.1, RFC1918, link-local, or IPv6 ULA/link-local addresses —
    // turning the renderer into a blind SSRF oracle against the host or
    // internal network. Loopback is permitted only under the test-only
    // `AllowLoopbackGuard` RAII bypass so tiny_http-backed tests still work.
    //
    // Redirects are followed manually (see loop below) with the same SSRF
    // check re-applied at every hop. ureq's built-in redirect follower
    // would short-circuit that invariant — a 302 to http://169.254.169.254
    // (cloud metadata) or http://10.0.0.1 would be invisible to the guard.
    const MAX_REDIRECTS: usize = 5;
    // Some CDNs (Cloudflare with bot protection, GitHub raw.*, a handful of
    // image hosts) 403 or 429 requests with an empty User-Agent. Identify
    // ourselves with a stable token that includes the crate version so
    // operators can allowlist or rate-limit the tool if needed.
    let user_agent = concat!("mat/", env!("CARGO_PKG_VERSION"));

    let mut current_url = url.to_string();
    let mut hops = 0usize;
    let resp = loop {
        // Parse URL (scheme / host / port) with the strict parser so ports,
        // userinfo, and IPv6 brackets are handled uniformly.
        let parsed = parse_http_url(&current_url).ok_or_else(|| "invalid url".to_string())?;
        // Resolve DNS and run every returned address through the policy
        // classifier before the socket is created.
        let ips = resolve_and_validate_host(&parsed.host)?;
        // Pin the resolution — wire ureq to dial *exactly* these IPs so a
        // second DNS lookup during connect cannot return a different
        // (possibly internal) address (DNS rebinding resistance).
        let pinned: Vec<SocketAddr> = ips
            .into_iter()
            .map(|ip| SocketAddr::new(ip, parsed.port))
            .collect();
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(5))
            .timeout_read(Duration::from_secs(15))
            .timeout_write(Duration::from_secs(15))
            // 0 ⇒ do not auto-follow; we do it in this loop so every
            // Location is re-validated and the scheme cannot silently
            // downgrade to http.
            .redirects(0)
            .resolver(AllowListResolver { addrs: pinned })
            .build();

        let r = match agent.get(&current_url).set("User-Agent", user_agent).call() {
            Ok(r) => r,
            Err(ureq::Error::Status(code, r)) => {
                // 3xx with `.redirects(0)` still surfaces through the error
                // path in ureq 2.x. Treat 301/302/303/307/308 as redirects
                // and fall through to the Location-handling branch below.
                if (300..400).contains(&code) {
                    r
                } else {
                    return Err(format!("http {code}"));
                }
            }
            Err(e) => return Err(format!("network: {e}")),
        };

        let status = r.status();
        if !(300..400).contains(&status) {
            break r;
        }
        // Redirect: validate hop count, extract Location, rewrite relative
        // references to absolute, forbid cross-scheme downgrade to http.
        hops += 1;
        if hops > MAX_REDIRECTS {
            return Err(format!("too many redirects (>{MAX_REDIRECTS})"));
        }
        let location = r
            .header("Location")
            .ok_or_else(|| format!("http {status} without Location header"))?
            .to_string();
        let next = resolve_redirect_target(&current_url, &location)
            .ok_or_else(|| format!("invalid redirect target: {location}"))?;
        if current_url.starts_with("https://") && next.starts_with("http://") {
            return Err("redirect downgrades https→http — refusing".to_string());
        }
        current_url = next;
    };

    if let Some(ct) = resp.header("Content-Type") {
        // Allow image/* or octet-stream; refuse everything else (e.g. text/html
        // would crash the decoder downstream).
        let ok = ct.starts_with("image/") || ct.starts_with("application/octet-stream");
        if !ok {
            return Err(format!("unsupported content-type: {ct}"));
        }
    }

    // Pick an extension hint from either Content-Type or URL path so the
    // image crate can sniff the right format.
    let ext = guess_image_extension(resp.header("Content-Type"), &current_url);
    let suffix = format!(".{ext}");
    let mut tf = tempfile::Builder::new()
        .prefix("mat-img-")
        .suffix(&suffix)
        .tempfile()
        .map_err(|e| format!("tempfile: {e}"))?;

    let mut reader = resp.into_reader().take(MAX_REMOTE_IMAGE_BYTES + 1);
    let copied = io::copy(&mut reader, &mut tf).map_err(|e| format!("read: {e}"))?;
    if copied > MAX_REMOTE_IMAGE_BYTES {
        return Err(format!("too large (>{MAX_REMOTE_IMAGE_BYTES} bytes)"));
    }
    if copied == 0 {
        return Err("empty response".to_string());
    }
    tf.flush().map_err(|e| format!("flush: {e}"))?;
    Ok(tf)
}

pub fn guess_image_extension(content_type: Option<&str>, url: &str) -> &'static str {
    if let Some(ct) = content_type {
        match ct.split(';').next().unwrap_or("").trim() {
            "image/png" => return "png",
            "image/jpeg" | "image/jpg" => return "jpg",
            "image/gif" => return "gif",
            "image/webp" => return "webp",
            _ => {}
        }
    }
    let path = url.split('?').next().unwrap_or(url);
    let lower = path.to_lowercase();
    if lower.ends_with(".png") {
        "png"
    } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "jpg"
    } else if lower.ends_with(".gif") {
        "gif"
    } else if lower.ends_with(".webp") {
        "webp"
    } else {
        "png"
    }
}
