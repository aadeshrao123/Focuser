/** Attribute each interval to the previously focused, unblocked page. A focus
 * change closes that interval; startup and sleep never backfill usage. */
export class SharedActivity {
  private previous: { url: string; at: number } | null = null;

  sample(url: string | null, now: number): { url: string; seconds: number } | null {
    const previous = this.previous;
    this.previous = url ? { url, at: now } : null;
    if (!previous) return null;
    const seconds = Math.floor((now - previous.at) / 1000);
    if (seconds < 1 || seconds > 60) return null;
    return { url: previous.url, seconds };
  }
}
