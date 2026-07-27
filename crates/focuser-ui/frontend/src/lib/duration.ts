/** Human-readable spans: "45s", "12m", "1h 24m". */
export function formatDuration(seconds: number): string {
  if (seconds < 60) return `${Math.round(seconds)}s`;

  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return `${minutes}m`;

  const remainder = minutes % 60;
  const hours = Math.floor(minutes / 60);
  return remainder ? `${hours}h ${remainder}m` : `${hours}h`;
}

/** Clock-style countdown: "24:13", or "1:04:09" once it passes an hour. */
export function formatCountdown(seconds: number): string {
  const total = Math.max(0, Math.floor(seconds));
  const hours = Math.floor(total / 3600);
  const minutes = Math.floor((total % 3600) / 60);
  const secs = total % 60;
  const pad = (n: number) => `${n}`.padStart(2, "0");

  return hours > 0 ? `${hours}:${pad(minutes)}:${pad(secs)}` : `${minutes}:${pad(secs)}`;
}
