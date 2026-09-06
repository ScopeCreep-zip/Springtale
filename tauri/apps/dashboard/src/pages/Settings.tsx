import type { Locale } from "@springtale/ui";
import { apiGet, apiLogin, apiPut, useI18n } from "@springtale/ui";
import { createSignal, onMount } from "solid-js";

/**
 * Settings page — configure API connection, auth, heartbeat, and language.
 *
 * Auth is a login, not a hash to paste (plan 6.6): the passphrase goes to
 * `POST /auth/login`, which returns a random session token the API client
 * keeps in module memory and renews on a 401. Nothing is derived here,
 * and neither the passphrase nor the token is ever persisted.
 *
 * Heartbeat config controls the periodic rule evaluation interval.
 * For IPV survivors: set a short interval to check safety contacts frequently.
 */
export function SettingsPage(props: { onSaved?: () => void }) {
  const { t, locale, setLocale } = useI18n();
  const [apiUrl, setApiUrl] = createSignal(`${window.location.protocol}//${window.location.host}`);
  const [passphrase, setPassphrase] = createSignal("");
  const [saved, setSaved] = createSignal(false);
  const [loginError, setLoginError] = createSignal("");
  const [heartbeatInterval, setHeartbeatInterval] = createSignal(1800);
  const [heartbeatEnabled, setHeartbeatEnabled] = createSignal(false);
  const [heartbeatSaved, setHeartbeatSaved] = createSignal(false);

  const saveConnection = async () => {
    setLoginError("");
    try {
      await apiLogin(apiUrl(), passphrase());
    } catch (e) {
      setLoginError(e instanceof Error ? e.message : String(e));
      return;
    } finally {
      // Clear the input either way — the passphrase lives only in the
      // API client's module memory, for re-login on a 401.
      setPassphrase("");
    }
    setSaved(true);
    setTimeout(() => setSaved(false), 2000);
    props.onSaved?.();
  };

  const fetchHeartbeat = async () => {
    try {
      const data = await apiGet<{ interval_secs: number; enabled: boolean }>("/config/heartbeat");
      setHeartbeatInterval(data.interval_secs);
      setHeartbeatEnabled(data.enabled);
    } catch {
      // Not connected yet — will load after auth is configured
    }
  };

  const saveHeartbeat = async () => {
    try {
      const data = await apiPut<{ interval_secs: number; enabled: boolean }>("/config/heartbeat", {
        interval_secs: heartbeatInterval(),
      });
      setHeartbeatInterval(data.interval_secs);
      setHeartbeatEnabled(data.enabled);
      setHeartbeatSaved(true);
      setTimeout(() => setHeartbeatSaved(false), 2000);
    } catch {
      // Handle error silently — user may not be authed yet
    }
  };

  onMount(fetchHeartbeat);

  return (
    <div>
      <div class="max-w-md space-y-6">
        <section aria-labelledby="connection-heading">
          <h2 id="connection-heading" class="text-lg font-semibold text-text-primary">
            {t("settings.connection")}
          </h2>
          <form
            onSubmit={(e) => {
              e.preventDefault();
              void saveConnection();
            }}
            class="mt-3 space-y-3"
          >
            <div>
              <label for="api-url" class="block text-sm font-medium text-text-secondary">
                {t("settings.apiUrl")}
              </label>
              <input
                id="api-url"
                type="text"
                value={apiUrl()}
                onInput={(e) => setApiUrl(e.currentTarget.value)}
                class="mt-1 w-full rounded border border-bark bg-soil-mid px-3 py-2 text-text-primary placeholder-text-dim focus:border-status-ok focus:outline-none"
                placeholder={t("settings.apiUrlPlaceholder")}
              />
            </div>
            <div>
              <label for="auth-passphrase" class="block text-sm font-medium text-text-secondary">
                {t("settings.passphrase")}
              </label>
              <input
                id="auth-passphrase"
                type="password"
                autocomplete="current-password"
                value={passphrase()}
                onInput={(e) => setPassphrase(e.currentTarget.value)}
                class="mt-1 w-full rounded border border-bark bg-soil-mid px-3 py-2 text-text-primary placeholder-text-dim focus:border-status-ok focus:outline-none"
                placeholder={t("settings.passphrasePlaceholder")}
                aria-describedby="auth-passphrase-help"
              />
              <p id="auth-passphrase-help" class="mt-1 text-xs text-text-dim">
                {t("settings.passphraseHelp")}
              </p>
              {loginError() && (
                <p role="alert" class="mt-1 text-xs text-status-error">
                  {loginError()}
                </p>
              )}
            </div>
            <button
              type="submit"
              class="rounded bg-status-ok px-4 py-2 text-sm font-medium text-text-primary hover:bg-canopy-highlight"
            >
              {saved() ? t("common.saved") : t("settings.saveConnection")}
            </button>
          </form>
        </section>

        <section aria-labelledby="heartbeat-heading">
          <h2 id="heartbeat-heading" class="text-lg font-semibold text-text-primary">
            {t("settings.heartbeat")}
          </h2>
          <p class="mt-1 text-sm text-text-dim">{t("settings.heartbeatDesc")}</p>
          <form
            onSubmit={(e) => {
              e.preventDefault();
              saveHeartbeat();
            }}
            class="mt-3 space-y-3"
          >
            <div>
              <label for="heartbeat-interval" class="block text-sm font-medium text-text-secondary">
                {t("settings.heartbeatInterval")}
              </label>
              <input
                id="heartbeat-interval"
                type="number"
                value={heartbeatInterval()}
                onInput={(e) => setHeartbeatInterval(parseInt(e.currentTarget.value, 10) || 0)}
                class="mt-1 w-full rounded border border-bark bg-soil-mid px-3 py-2 text-text-primary placeholder-text-dim focus:border-status-ok focus:outline-none"
                placeholder="1800"
                min="0"
                aria-describedby="heartbeat-help"
              />
              <p id="heartbeat-help" class="mt-1 text-xs text-text-dim">
                {t("settings.heartbeatDefault")}{" "}
                {heartbeatEnabled()
                  ? t("settings.heartbeatRunning")
                  : t("settings.heartbeatStopped")}
              </p>
            </div>
            <button
              type="submit"
              class="rounded bg-status-ok px-4 py-2 text-sm font-medium text-text-primary hover:bg-canopy-highlight"
            >
              {heartbeatSaved() ? t("common.saved") : t("settings.saveHeartbeat")}
            </button>
          </form>
        </section>

        <section aria-labelledby="language-heading">
          <h2 id="language-heading" class="text-lg font-semibold text-text-primary">
            {t("settings.language")}
          </h2>
          <div class="mt-3">
            <label for="language-select" class="sr-only">
              {t("settings.language")}
            </label>
            <select
              id="language-select"
              value={locale()}
              onChange={(e) => setLocale(e.currentTarget.value as Locale)}
              class="w-full rounded border border-bark bg-soil-mid px-3 py-2 text-text-primary"
            >
              <option value="en">English</option>
              <option value="es">Español</option>
              <option value="pt">Português</option>
              <option value="fr">Français</option>
              <option value="ar">العربية</option>
              <option value="th">ไทย</option>
              <option value="tl">Tagalog</option>
              <option value="ja">日本語</option>
            </select>
          </div>
        </section>
      </div>
    </div>
  );
}
