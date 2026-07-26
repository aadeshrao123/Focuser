/**
 * Temporary route body used while the command core is being ported.
 *
 * Each of these is replaced by a real page during Track A; the shell exists
 * first so routing, theming, and the query client can be verified end to end
 * before any feature work lands.
 */
export function Placeholder({ title }: { title: string }) {
  return (
    <div className="p-8">
      <h1 className="font-semibold text-2xl text-foreground tracking-tight">{title}</h1>
      <p className="mt-2 text-muted-foreground text-sm">
        Not built yet — waiting on the typed command bindings.
      </p>
    </div>
  );
}
