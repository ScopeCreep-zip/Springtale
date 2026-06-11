/**
 * HTTP client for springtaled management API.
 *
 * All requests use Bearer token in Authorization header.
 * No cookies, no sessions — stateless auth per ARCHITECTURE.md §9.
 */

/** Default API URL — same host, management API port. */
const DEFAULT_BASE_URL =
  typeof window !== "undefined"
    ? `${window.location.protocol}//${window.location.host}`
    : "http://127.0.0.1:8080";

let baseUrl = DEFAULT_BASE_URL;
let token = "";

/** Configure the API client with base URL and auth token. */
export function configure(url: string, authToken: string): void {
  baseUrl = url;
  token = authToken;
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

/** Make an authenticated GET request. */
export async function get<T>(path: string): Promise<T> {
  const response = await fetch(`${baseUrl}${path}`, {
    headers: { Authorization: `Bearer ${token}` },
  });
  if (!response.ok) {
    throw await errorFromResponse(response);
  }
  return response.json() as Promise<T>;
}

/** Make an authenticated POST request with JSON body. */
export async function post<T>(path: string, body?: unknown): Promise<T> {
  const response = await fetch(`${baseUrl}${path}`, {
    method: "POST",
    headers: {
      Authorization: `Bearer ${token}`,
      "Content-Type": "application/json",
    },
    body: body ? JSON.stringify(body) : undefined,
  });
  if (!response.ok) {
    throw await errorFromResponse(response);
  }
  return response.json() as Promise<T>;
}

/** Make an authenticated PUT request with JSON body. */
export async function put<T>(path: string, body: unknown): Promise<T> {
  const response = await fetch(`${baseUrl}${path}`, {
    method: "PUT",
    headers: {
      Authorization: `Bearer ${token}`,
      "Content-Type": "application/json",
    },
    body: JSON.stringify(body),
  });
  if (!response.ok) {
    throw await errorFromResponse(response);
  }
  return response.json() as Promise<T>;
}

/** Make an authenticated DELETE request. */
export async function del<T = void>(path: string): Promise<T> {
  const response = await fetch(`${baseUrl}${path}`, {
    method: "DELETE",
    headers: { Authorization: `Bearer ${token}` },
  });
  if (!response.ok) {
    throw await errorFromResponse(response);
  }
  const text = await response.text();
  if (!text) return undefined as T;
  return JSON.parse(text) as T;
}
