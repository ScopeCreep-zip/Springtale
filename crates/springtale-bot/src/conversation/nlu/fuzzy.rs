//! Pure-Rust fuzzy string similarity for typo tolerance.
//!
//! Two small, well-known algorithms — no external crate (keeps the
//! `#![forbid(unsafe_code)]` surface trivial and avoids a new
//! dependency per the workspace dependency rules). `jaro_winkler` is
//! the primary signal for intent typo tolerance ("wether" → "weather");
//! `damerau_levenshtein` backs the short-token gazetteer fallback where
//! transpositions are common ("tuscon" → "tucson").

/// Jaro–Winkler similarity in `[0.0, 1.0]`. 1.0 = identical.
pub fn jaro_winkler(a: &str, b: &str) -> f64 {
    let jaro = jaro(a, b);
    if jaro < 0.7 {
        return jaro;
    }
    // Winkler boost for a shared prefix (up to 4 chars), p = 0.1.
    let prefix = a
        .chars()
        .zip(b.chars())
        .take(4)
        .take_while(|(x, y)| x == y)
        .count() as f64;
    jaro + prefix * 0.1 * (1.0 - jaro)
}

fn jaro(a: &str, b: &str) -> f64 {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }

    let max_dist = (a.len().max(b.len()) / 2).saturating_sub(1);
    let mut a_match = vec![false; a.len()];
    let mut b_match = vec![false; b.len()];
    let mut matches = 0usize;

    for (i, &ca) in a.iter().enumerate() {
        let start = i.saturating_sub(max_dist);
        let end = (i + max_dist + 1).min(b.len());
        for j in start..end {
            if !b_match[j] && b[j] == ca {
                a_match[i] = true;
                b_match[j] = true;
                matches += 1;
                break;
            }
        }
    }
    if matches == 0 {
        return 0.0;
    }

    // Count transpositions.
    let mut transpositions = 0usize;
    let mut k = 0usize;
    for i in 0..a.len() {
        if a_match[i] {
            while !b_match[k] {
                k += 1;
            }
            if a[i] != b[k] {
                transpositions += 1;
            }
            k += 1;
        }
    }
    let m = matches as f64;
    let t = (transpositions / 2) as f64;
    (m / a.len() as f64 + m / b.len() as f64 + (m - t) / m) / 3.0
}

/// Damerau–Levenshtein edit distance (insert / delete / substitute /
/// adjacent transposition). Lower = closer.
pub fn damerau_levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let (n, m) = (a.len(), b.len());
    if n == 0 {
        return m;
    }
    if m == 0 {
        return n;
    }

    let mut prev_prev = vec![0usize; m + 1];
    let mut prev: Vec<usize> = (0..=m).collect();
    let mut curr = vec![0usize; m + 1];

    for i in 1..=n {
        curr[0] = i;
        for j in 1..=m {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            let mut val = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                val = val.min(prev_prev[j - 2] + 1);
            }
            curr[j] = val;
        }
        prev_prev.clone_from(&prev);
        prev.clone_from(&curr);
    }
    prev[m]
}

/// Convenience: are two short tokens within `max_edits` transposition-
/// aware edits of each other? Used for gazetteer typo tolerance.
pub fn close_enough(a: &str, b: &str, max_edits: usize) -> bool {
    // Cheap length-gate before the O(n·m) walk.
    if a.len().abs_diff(b.len()) > max_edits {
        return false;
    }
    damerau_levenshtein(a, b) <= max_edits
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_jaro_winkler_identical_is_one() {
        assert!((jaro_winkler("weather", "weather") - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_jaro_winkler_typo_is_high() {
        // single-letter omission should score well above the 0.90 floor
        assert!(jaro_winkler("wether", "weather") > 0.90);
        assert!(jaro_winkler("remind", "reminder") > 0.90);
    }

    #[test]
    fn test_jaro_winkler_unrelated_stays_below_fuzzy_floor() {
        // The intent matcher only counts a fuzzy hit at ≥0.90. Unrelated
        // words must stay clearly under that AND below a real typo's score.
        let unrelated = jaro_winkler("weather", "telegram");
        assert!(unrelated < 0.90, "unrelated scored {unrelated}");
        assert!(unrelated < jaro_winkler("wether", "weather"));
    }

    #[test]
    fn test_damerau_handles_transposition() {
        // "tuscon" vs "tucson" is ONE adjacent transposition.
        assert_eq!(damerau_levenshtein("tuscon", "tucson"), 1);
    }

    #[test]
    fn test_close_enough_gate() {
        assert!(close_enough("tuscon", "tucson", 1));
        assert!(!close_enough("tucson", "seattle", 1));
    }
}
