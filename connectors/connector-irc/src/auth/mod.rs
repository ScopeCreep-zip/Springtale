// IRC authentication is handled by the `irc` crate's Config.
// When `nick_password` is set in the config, the crate handles
// NickServ IDENTIFY automatically after connection.
//
// SASL PLAIN is supported via `client.send_sasl_plain()` for
// servers that require it (e.g., Libera.Chat with account-only mode).
//
// This module exists for the connector structure convention
// but delegates all auth to the irc crate.
