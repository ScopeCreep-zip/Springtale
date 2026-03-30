use super::RouteResult;

/// Fallback router: returns `NoMatch` with a help suggestion.
pub struct FallbackRouter;

impl FallbackRouter {
    /// Return a no-match result with a helpful suggestion.
    pub fn no_match(prefix_char: char) -> RouteResult {
        RouteResult::NoMatch {
            suggestion: format!("Unknown command. Try {prefix_char}help for a list of commands."),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_fallback_contains_help() {
        let result = FallbackRouter::no_match('/');
        match result {
            RouteResult::NoMatch { suggestion } => {
                assert!(suggestion.contains("/help"));
            }
            RouteResult::Command { .. } => panic!("expected NoMatch"),
        }
    }

    #[test]
    fn test_fallback_custom_prefix() {
        let result = FallbackRouter::no_match('!');
        match result {
            RouteResult::NoMatch { suggestion } => {
                assert!(suggestion.contains("!help"));
            }
            RouteResult::Command { .. } => panic!("expected NoMatch"),
        }
    }
}
