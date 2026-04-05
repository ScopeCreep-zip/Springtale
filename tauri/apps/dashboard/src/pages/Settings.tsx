import { createSignal, onMount } from "solid-js";
import { useI18n } from "@springtale/ui";
import type { Locale } from "@springtale/ui";
import { configure, get, put } from "../api/client";

/**
 * Settings page — configure API connection, auth, heartbeat, and language.
 *
 * The auth token is the hex-encoded HMAC-SHA256 hash derived from the
 * vault passphrase. Users compute it with:
 *   springtale-cli token
 * or derive it manually from their vault passphrase.
 *
 * Heartbeat config controls the periodic rule evaluation interval.
 * For IPV survivors: set a short interval to check safety contacts frequently.
 */
export function SettingsPage(props: { onSaved?: () => void }) {
  const { t, locale, setLocale } = useI18n();
  const [apiUrl, setApiUrl] = createSignal(
    `${window.location.protocol}//${window.location.host}`,
  );
  const [token, setToken] = createSignal("");
  const [saved, setSaved] = createSignal(false);
  const [heartbeatInterval, setHeartbeatInterval] = createSignal(1800);
  const [heartbeatEnabled, setHeartbeatEnabled] = createSignal(false);
  const [heartbeatSaved, setHeartbeatSaved] = createSignal(false);

  const saveConnection = () => {
    configure(apiUrl(), token());
    setToken(""); // Clear token signal — only kept in client module memory
    setSaved(true);
    setTimeout(() => setSaved(false), 2000);
    props.onSaved?.();
  };

  const fetchHeartbeat = async () => {
    try {
      const data = await get<{ interval_secs: number; enabled: boolean }>(
        "/config/heartbeat",
      );
      setHeartbeatInterval(data.interval_secs);
      setHeartbeatEnabled(data.enabled);
    } catch {
      // Not connected yet — will load after auth is configured
    }
  };

  const saveHeartbeat = async () => {
    try {
      const data = await put<{ interval_secs: number; enabled: boolean }>(
        "/config/heartbeat",
        { interval_secs: heartbeatInterval() },
      );
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
          <h2 id="connection-heading" class="text-lg font-semibold text-gray-200">
            {t("settings.connection")}
          </h2>
          <form onSubmit={(e) => { e.preventDefault(); saveConnection(); }} class="mt-3 space-y-3">
            <div>
              <label for="api-url" class="block text-sm font-medium text-gray-300">
                {t("settings.apiUrl")}
              </label>
              <input
                id="api-url"
                type="text"
                value={apiUrl()}
                onInput={(e) => setApiUrl(e.currentTarget.value)}
                class="mt-1 w-full rounded border border-gray-700 bg-gray-800 px-3 py-2 text-white placeholder-gray-500 focus:border-blue-500 focus:outline-none"
                placeholder={t("settings.apiUrlPlaceholder")}
              />
            </div>
            <div>
              <label for="auth-token" class="block text-sm font-medium text-gray-300">
                {t("settings.authToken")}
              </label>
              <input
                id="auth-token"
                type="password"
                value={token()}
                onInput={(e) => setToken(e.currentTarget.value)}
                class="mt-1 w-full rounded border border-gray-700 bg-gray-800 px-3 py-2 text-white placeholder-gray-500 focus:border-blue-500 focus:outline-none"
                placeholder={t("settings.authTokenPlaceholder")}
                aria-describedby="auth-token-help"
              />
              <p id="auth-token-help" class="mt-1 text-xs text-gray-500">
                {t("settings.authTokenHelp")}
              </p>
            </div>
            <button
              type="submit"
              class="rounded bg-blue-600 px-4 py-2 text-sm font-medium text-white hover:bg-blue-500"
            >
              {saved() ? t("common.saved") : t("settings.saveConnection")}
            </button>
          </form>
        </section>

        <section aria-labelledby="heartbeat-heading">
          <h2 id="heartbeat-heading" class="text-lg font-semibold text-gray-200">
            {t("settings.heartbeat")}
          </h2>
          <p class="mt-1 text-sm text-gray-400">
            {t("settings.heartbeatDesc")}
          </p>
          <form onSubmit={(e) => { e.preventDefault(); saveHeartbeat(); }} class="mt-3 space-y-3">
            <div>
              <label for="heartbeat-interval" class="block text-sm font-medium text-gray-300">
                {t("settings.heartbeatInterval")}
              </label>
              <input
                id="heartbeat-interval"
                type="number"
                value={heartbeatInterval()}
                onInput={(e) =>
                  setHeartbeatInterval(parseInt(e.currentTarget.value) || 0)
                }
                class="mt-1 w-full rounded border border-gray-700 bg-gray-800 px-3 py-2 text-white placeholder-gray-500 focus:border-blue-500 focus:outline-none"
                placeholder="1800"
                min="0"
                aria-describedby="heartbeat-help"
              />
              <p id="heartbeat-help" class="mt-1 text-xs text-gray-500">
                {t("settings.heartbeatDefault")}
                {" "}
                {heartbeatEnabled()
                  ? t("settings.heartbeatRunning")
                  : t("settings.heartbeatStopped")}
              </p>
            </div>
            <button
              type="submit"
              class="rounded bg-blue-600 px-4 py-2 text-sm font-medium text-white hover:bg-blue-500"
            >
              {heartbeatSaved() ? t("common.saved") : t("settings.saveHeartbeat")}
            </button>
          </form>
        </section>

        <section aria-labelledby="language-heading">
          <h2 id="language-heading" class="text-lg font-semibold text-gray-200">
            {t("settings.language")}
          </h2>
          <div class="mt-3">
            <label for="language-select" class="sr-only">{t("settings.language")}</label>
            <select
              id="language-select"
              value={locale()}
              onChange={(e) => setLocale(e.currentTarget.value as Locale)}
              class="w-full rounded border border-gray-700 bg-gray-800 px-3 py-2 text-white"
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
