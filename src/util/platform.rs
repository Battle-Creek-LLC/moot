//! Platform detection from a meeting URL.
//!
//! See SPEC §5.1: `meet.google.com` → meet, `teams.microsoft.com|teams.live.com`
//! → teams, `*.zoom.us` → zoom, anything else → unknown.

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Platform {
    Meet,
    Teams,
    Zoom,
    Unknown,
}

impl Platform {
    pub fn as_str(&self) -> &'static str {
        match self {
            Platform::Meet => "meet",
            Platform::Teams => "teams",
            Platform::Zoom => "zoom",
            Platform::Unknown => "unknown",
        }
    }
}

/// Best-effort hostname-based detection. Falls back to `Unknown` for malformed
/// URLs or unrecognized hosts.
pub fn detect(url: &str) -> Platform {
    let host = match host_of(url) {
        Some(h) => h.to_ascii_lowercase(),
        None => return Platform::Unknown,
    };

    if host == "meet.google.com" {
        Platform::Meet
    } else if host == "teams.microsoft.com" || host == "teams.live.com" {
        Platform::Teams
    } else if host.ends_with(".zoom.us") || host == "zoom.us" {
        Platform::Zoom
    } else {
        Platform::Unknown
    }
}

fn host_of(url: &str) -> Option<&str> {
    let after_scheme = url.split_once("://").map(|(_, rest)| rest).unwrap_or(url);
    let host = after_scheme.split(['/', '?', '#']).next()?;
    let host = host.split('@').next_back()?;
    let host = host.rsplit_once(':').map(|(h, _)| h).unwrap_or(host);
    if host.is_empty() { None } else { Some(host) }
}

/// Parse one of the four platform tags accepted by CLI flags.
pub fn parse(s: &str) -> Option<Platform> {
    match s.trim().to_ascii_lowercase().as_str() {
        "meet" => Some(Platform::Meet),
        "teams" => Some(Platform::Teams),
        "zoom" => Some(Platform::Zoom),
        "unknown" => Some(Platform::Unknown),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_meet() {
        assert_eq!(detect("https://meet.google.com/abc-defg-hij"), Platform::Meet);
    }

    #[test]
    fn detects_teams() {
        assert_eq!(detect("https://teams.microsoft.com/l/meetup-join/..."), Platform::Teams);
        assert_eq!(detect("https://teams.live.com/meet/..."), Platform::Teams);
    }

    #[test]
    fn detects_zoom() {
        assert_eq!(detect("https://us02web.zoom.us/j/123"), Platform::Zoom);
        assert_eq!(detect("https://zoom.us/j/123"), Platform::Zoom);
    }

    #[test]
    fn unknown_fallback() {
        assert_eq!(detect("https://example.com/meeting"), Platform::Unknown);
        assert_eq!(detect(""), Platform::Unknown);
        assert_eq!(detect("not-a-url"), Platform::Unknown);
    }

    #[test]
    fn handles_userinfo_and_port() {
        assert_eq!(detect("https://user:pass@meet.google.com:443/foo"), Platform::Meet);
    }
}
