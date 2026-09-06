/**
 * Colour for the backend's four health states
 * (`healthy | degraded | incapacitated | dead`). `healthy` takes the
 * caller's accent so bars keep their own palette.
 */
export function healthColor(state: string | undefined, healthy = "var(--color-status-ok)"): string {
  switch (state) {
    case "healthy":
      return healthy;
    case "degraded":
      return "var(--color-status-warn)";
    case "incapacitated":
      return "var(--color-status-error)";
    default:
      return "var(--color-text-dim)";
  }
}
