/**
 * i18n context — provides translations to all components.
 *
 * Per ARCHITECTURE.md §16.2: "SolidJS i18n via @solid-primitives/i18n."
 *
 * Uses flat key→string dictionaries loaded lazily per locale.
 * Template interpolation via {{key}} syntax.
 * RTL detection for Arabic (the only RTL priority language).
 */

import * as i18n from "@solid-primitives/i18n";
import { createContext, createEffect, createResource, createSignal, useContext } from "solid-js";
import type { Locale, RawDictionary } from "./types";

const RTL_LOCALES: Locale[] = ["ar"];

async function loadDictionary(locale: Locale): Promise<RawDictionary> {
  const mod = await import(`./locales/${locale}.json`);
  return mod.default as RawDictionary;
}

export function createI18n(initial: Locale = "en") {
  const [locale, setLocale] = createSignal<Locale>(initial);
  const [dict] = createResource(locale, loadDictionary);

  const t = i18n.translator(() => (dict() ?? {}) as Record<string, string>, i18n.resolveTemplate);

  const isRTL = () => RTL_LOCALES.includes(locale());
  /** Text direction of the current locale, for glyphs that mirror under RTL. */
  const dir = (): "rtl" | "ltr" => (isRTL() ? "rtl" : "ltr");

  // Reactively update document direction and lang attribute
  createEffect(() => {
    document.documentElement.lang = locale();
    document.documentElement.dir = isRTL() ? "rtl" : "ltr";
  });

  return { t, locale, setLocale, isRTL, dir };
}

// Context for tree-wide access
const I18nContext = createContext<ReturnType<typeof createI18n>>();

export const I18nProvider = I18nContext.Provider;

export function useI18n() {
  const ctx = useContext(I18nContext);
  if (!ctx) {
    throw new Error("useI18n must be used within I18nProvider");
  }
  return ctx;
}
