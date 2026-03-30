use std::collections::HashMap;

/// Resolves user-defined command aliases.
/// Loaded from SQLite at startup, updated on `/alias set` commands.
pub struct AliasResolver {
    /// Maps alias → target command name.
    aliases: HashMap<String, String>,
}

impl AliasResolver {
    pub fn new(aliases: HashMap<String, String>) -> Self {
        Self { aliases }
    }

    /// Try to resolve an alias in the text.
    /// If the text starts with `{prefix}{alias}`, returns the text with
    /// the alias replaced by the target command.
    ///
    /// Example: `/s tokyo` with alias `s` → `search` returns `/search tokyo`.
    pub fn resolve(&self, text: &str, prefix_char: char) -> Option<String> {
        let trimmed = text.trim();
        if !trimmed.starts_with(prefix_char) {
            return None;
        }

        let without_prefix = &trimmed[prefix_char.len_utf8()..];
        let (alias_part, rest) = match without_prefix.split_once(' ') {
            Some((a, r)) => (a, Some(r)),
            None => (without_prefix, None),
        };

        let alias_lower = alias_part.to_lowercase();
        if let Some(target) = self.aliases.get(&alias_lower) {
            let mut resolved = format!("{prefix_char}{target}");
            if let Some(args) = rest {
                resolved.push(' ');
                resolved.push_str(args);
            }
            Some(resolved)
        } else {
            None
        }
    }

    /// Set an alias mapping.
    pub fn set(&mut self, alias: &str, target: &str) {
        self.aliases
            .insert(alias.to_lowercase(), target.to_lowercase());
    }

    /// Remove an alias.
    pub fn remove(&mut self, alias: &str) {
        self.aliases.remove(&alias.to_lowercase());
    }

    /// List all aliases.
    pub fn list(&self) -> &HashMap<String, String> {
        &self.aliases
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_alias_resolve() {
        let mut aliases = HashMap::new();
        aliases.insert("s".into(), "search".into());
        let resolver = AliasResolver::new(aliases);

        let result = resolver.resolve("/s tokyo", '/');
        assert_eq!(result, Some("/search tokyo".into()));
    }

    #[test]
    fn test_alias_resolve_no_args() {
        let mut aliases = HashMap::new();
        aliases.insert("h".into(), "help".into());
        let resolver = AliasResolver::new(aliases);

        let result = resolver.resolve("/h", '/');
        assert_eq!(result, Some("/help".into()));
    }

    #[test]
    fn test_alias_no_match() {
        let resolver = AliasResolver::new(HashMap::new());
        assert!(resolver.resolve("/search foo", '/').is_none());
    }

    #[test]
    fn test_alias_no_prefix() {
        let mut aliases = HashMap::new();
        aliases.insert("s".into(), "search".into());
        let resolver = AliasResolver::new(aliases);
        assert!(resolver.resolve("s foo", '/').is_none());
    }

    #[test]
    fn test_alias_case_insensitive() {
        let mut aliases = HashMap::new();
        aliases.insert("s".into(), "search".into());
        let resolver = AliasResolver::new(aliases);

        let result = resolver.resolve("/S tokyo", '/');
        assert_eq!(result, Some("/search tokyo".into()));
    }

    #[test]
    fn test_alias_set_and_remove() {
        let mut resolver = AliasResolver::new(HashMap::new());
        resolver.set("s", "search");
        assert!(resolver.resolve("/s test", '/').is_some());

        resolver.remove("s");
        assert!(resolver.resolve("/s test", '/').is_none());
    }
}
