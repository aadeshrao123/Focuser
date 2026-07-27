import { cn } from "@/lib/utils";

/**
 * A tile standing in for a site or application.
 *
 * Deliberately *not* a real favicon. Fetching one means asking a third party
 * (Google's favicon endpoint, or the site itself) for every entry in the list,
 * which would hand over the user's entire block list — the exact thing this app
 * promises never to leave the machine. A monogram tinted by the name is free,
 * offline, and stable: the same domain always gets the same colour.
 */
const HUES = [265, 190, 45, 330, 155, 15, 220, 290];

function hueFor(value: string): number {
  // djb2, for a stable spread across the palette.
  let hash = 5381;
  for (let i = 0; i < value.length; i++) hash = (hash * 33) ^ value.charCodeAt(i);
  return HUES[Math.abs(hash) % HUES.length];
}

/** "www.youtube.com" → "Y", "steam.exe" → "S". */
function initialFor(value: string): string {
  const cleaned = value
    .replace(/^https?:\/\//, "")
    .replace(/^www\./, "")
    .replace(/\.(exe|app|sh|appimage)$/i, "");
  return (cleaned.match(/[a-z0-9]/i)?.[0] ?? "?").toUpperCase();
}

export function TargetIcon({ value, className }: { value: string; className?: string }) {
  const hue = hueFor(value);

  return (
    <span
      aria-hidden
      className={cn(
        "flex size-8 shrink-0 select-none items-center justify-center rounded-lg font-semibold text-xs",
        "ring-1 ring-inset",
        className,
      )}
      style={{
        backgroundColor: `oklch(0.32 0.07 ${hue})`,
        color: `oklch(0.86 0.13 ${hue})`,
        // biome-ignore lint/style/useNamingConvention: CSS custom property
        ["--tw-ring-color" as string]: `oklch(0.5 0.09 ${hue} / 0.35)`,
      }}
    >
      {initialFor(value)}
    </span>
  );
}
