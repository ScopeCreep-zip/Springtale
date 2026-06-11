#![no_main]

use libfuzzer_sys::fuzz_target;

// NetworkOutbound capability allowlist match fuzz.
//
// Connector manifests declare exact-host allow-lists for outbound HTTP.
// The runtime must accept inputs that match the declared host exactly
// and reject anything else. This target fuzzes the match function against
// arbitrary URL-shaped strings.

fn host_matches(allow: &str, candidate: &str) -> bool {
    match url::Url::parse(candidate) {
        Ok(u) => u.host_str() == Some(allow),
        Err(_) => false,
    }
}

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        // Two halves: allow-listed host + candidate URL.
        if let Some(idx) = s.find('|') {
            let (allow, candidate) = s.split_at(idx);
            let candidate = &candidate[1..];
            let _ = host_matches(allow, candidate);
        }
    }
});
