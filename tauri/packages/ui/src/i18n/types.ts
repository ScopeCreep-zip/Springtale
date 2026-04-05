/**
 * i18n type definitions.
 *
 * Priority languages per ARCHITECTURE.md §16.2:
 * English, Spanish, Portuguese, French, Arabic, Thai, Tagalog, Japanese.
 */

export type Locale = "en" | "es" | "pt" | "fr" | "ar" | "th" | "tl" | "ja";

export type RawDictionary = Record<string, string>;
