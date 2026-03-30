pub mod alias;
pub mod fallback;
pub mod pattern;
pub mod prefix;

pub use alias::AliasResolver;
pub use fallback::FallbackRouter;
pub use pattern::{PatternEntry, PatternRouter};
pub use prefix::PrefixRouter;

use std::collections::HashMap;

/// Result of routing a message through the command router.
/// Pure data — no side effects, no async.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouteResult {
    /// Matched a command. `name` is the canonical command name
    /// (after alias resolution). `args` is the remaining text.
    Command { name: String, args: String },

    /// No command matched.
    NoMatch { suggestion: String },
}

/// The command router. A pure function: `(text) -> RouteResult`.
/// No async, no side effects, no state mutation during routing.
///
/// Resolution order: alias → prefix → pattern → fallback.
pub struct Router {
    prefix: PrefixRouter,
    pattern: PatternRouter,
    alias: AliasResolver,
}

impl Router {
    /// Create a router with the given aliases and patterns.
    pub fn new(aliases: HashMap<String, String>, patterns: Vec<PatternEntry>) -> Self {
        Self {
            prefix: PrefixRouter::new(),
            pattern: PatternRouter::new(patterns),
            alias: AliasResolver::new(aliases),
        }
    }

    /// Register a command name with the prefix router.
    pub fn register_command(&mut self, command: &str) {
        self.prefix.register(command);
    }

    /// Unregister a command name from the prefix router.
    pub fn unregister_command(&mut self, command: &str) {
        self.prefix.unregister(command);
    }

    /// Get a mutable reference to the alias resolver.
    pub fn aliases_mut(&mut self) -> &mut AliasResolver {
        &mut self.alias
    }

    /// Get a mutable reference to the prefix router (for auto-registration).
    pub fn prefix_mut(&mut self) -> &mut PrefixRouter {
        &mut self.prefix
    }

    /// Route a message. Pure function — does not mutate state.
    pub fn route(&self, text: &str, prefix_char: char) -> RouteResult {
        // 1. Try alias resolution first
        let resolved = self.alias.resolve(text, prefix_char);
        let input = resolved.as_deref().unwrap_or(text);

        // 2. Try prefix match
        if let Some((name, args)) = self.prefix.try_match(input, prefix_char) {
            return RouteResult::Command { name, args };
        }

        // 3. Try pattern match
        if let Some((name, args)) = self.pattern.try_match(input) {
            return RouteResult::Command { name, args };
        }

        // 4. Fallback
        FallbackRouter::no_match(prefix_char)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_route_prefix_match() {
        let mut router = Router::new(HashMap::new(), vec![]);
        router.register_command("help");
        router.register_command("search");

        let result = router.route("/help", '/');
        assert_eq!(
            result,
            RouteResult::Command {
                name: "help".into(),
                args: String::new(),
            }
        );
    }

    #[test]
    fn test_route_prefix_with_args() {
        let mut router = Router::new(HashMap::new(), vec![]);
        router.register_command("search");

        let result = router.route("/search tokyo weather", '/');
        assert_eq!(
            result,
            RouteResult::Command {
                name: "search".into(),
                args: "tokyo weather".into(),
            }
        );
    }

    #[test]
    fn test_route_alias_resolution() {
        let mut aliases = HashMap::new();
        aliases.insert("s".into(), "search".into());
        let mut router = Router::new(aliases, vec![]);
        router.register_command("search");

        let result = router.route("/s tokyo", '/');
        assert_eq!(
            result,
            RouteResult::Command {
                name: "search".into(),
                args: "tokyo".into(),
            }
        );
    }

    #[test]
    fn test_route_no_match() {
        let router = Router::new(HashMap::new(), vec![]);
        let result = router.route("random text", '/');
        assert!(matches!(result, RouteResult::NoMatch { .. }));
    }

    #[test]
    fn test_route_unknown_command() {
        let router = Router::new(HashMap::new(), vec![]);
        let result = router.route("/nonexistent foo", '/');
        assert!(matches!(result, RouteResult::NoMatch { .. }));
    }
}
