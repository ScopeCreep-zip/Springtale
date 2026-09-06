/**
 * HTTP client for springtaled management API.
 *
 * All requests use a Bearer token in the Authorization header. The token
 * is one the daemon ISSUED (plan 6.6): the browser posts the vault
 * passphrase to `/auth/login` once and gets back a random session token.
 * Nothing is hashed or HMAC'd here — the browser has no business
 * deriving a credential, and the old "paste the HMAC of your passphrase"
 * flow is gone.
 *
 * The passphrase and the token live in module memory only: never
 * localStorage, never a cookie, never a URL. A session that has aged out
 * of its idle window answers 401, and the client silently logs in again
 * with the passphrase it still holds — OWASP's mandatory session-ID
 * regeneration at authentication, once per re-login.
 */

/** Default API URL — same host, management API port. */
const DEFAULT_BASE_URL =
  typeof window !== "undefined"
    ? `${window.location.protocol}//${window.location.host}`
    : "http://127.0.0.1:8080";

let baseUrl = DEFAULT_BASE_URL;
let token = "";
/** Vault passphrase, module memory only — used to re-login on a 401. */
let passphrase = "";

/**
 * Configure the API client with a base URL and a token the daemon
 * already issued (the desktop shell's path — it logs in over IPC and
 * hands the token straight here).
 */
export function configure(url: string, authToken: string): void {
  baseUrl = url;
  token = authToken;
  passphrase = "";
}

/**
 * Log in with the vault passphrase and hold the issued token.
 *
 * The passphrase is kept in module memory so an expired session can be
 * renewed without re-prompting; `logout()` drops both.
 */
export async function login(url: string, vaultPassphrase: string): Promise<void> {
  baseUrl = url;
  passphrase = vaultPassphrase;
  await mintToken();
}

/** Forget the token and the passphrase. */
export function logout(): void {
  token = "";
  passphrase = "";
}

/** Exchange the held passphrase for a fresh session token. */
async function mintToken(): Promise<boolean> {
  if (!passphrase) return false;
  const response = await fetch(`${baseUrl}/auth/login`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ passphrase }),
  });
  if (!response.ok) {
    token = "";
    if (response.status === 401) {
      passphrase = "";
      throw new Error("Passphrase rejected");
    }
    throw await errorFromResponse(response);
  }
  const body = (await response.json()) as { token?: string };
  token = body.token ?? "";
  return token !== "";
}

/** Get the current base URL (for SSE client). */
export function getBaseUrl(): string {
  return baseUrl;
}

/** Get the current auth token (for SSE client). */
export function getToken(): string {
  return token;
}

/**
 * Build an Error from a failed response, preferring the server's response
 * BODY (the daemon returns the real message there, e.g. "I couldn't find a
 * place called 'Sacramento, XYZ'") and falling back to the status line.
 */
async function errorFromResponse(response: Response): Promise<Error> {
  let detail = "";
  try {
    detail = (await response.text()).trim();
  } catch {
    // body unreadable — fall back to the status line below
  }
  return new Error(detail || `API error: ${response.status} ${response.statusText}`);
}

/**
 * Send one authenticated request, re-logging in exactly once if the
 * session has expired or been revoked. One retry, never a loop: if the
 * fresh token is also refused, the caller sees the 401.
 */
async function send<T>(path: string, init: RequestInit, retry = true): Promise<T> {
  const headers: Record<string, string> = {
    ...((init.headers as Record<string, string> | undefined) ?? {}),
    Authorization: `Bearer ${token}`,
  };
  const response = await fetch(`${baseUrl}${path}`, { ...init, headers });
  if (response.status === 401 && retry && (await mintToken())) {
    return send<T>(path, init, false);
  }
  if (!response.ok) {
    throw await errorFromResponse(response);
  }
  const text = await response.text();
  if (!text) return undefined as T;
  return JSON.parse(text) as T;
}

/** Make an authenticated GET request. */
export async function get<T>(path: string): Promise<T> {
  return send<T>(path, { method: "GET" });
}

/** Make an authenticated POST request with JSON body. */
export async function post<T>(path: string, body?: unknown): Promise<T> {
  return send<T>(path, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: body ? JSON.stringify(body) : undefined,
  });
}

/** Make an authenticated PUT request with JSON body. */
export async function put<T>(path: string, body: unknown): Promise<T> {
  return send<T>(path, {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
}

/** Make an authenticated DELETE request. */
export async function del<T = void>(path: string): Promise<T> {
  return send<T>(path, { method: "DELETE" });
}
