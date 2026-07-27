import type { ReactNode } from "react";
import { cn } from "@/lib/utils";

/**
 * A tile standing in for a site or application.
 *
 * Deliberately *not* a real favicon. Fetching one means asking a third party
 * (Google's favicon endpoint, or the site itself) for every entry in the list,
 * which would hand over the user's entire block list — the exact thing this app
 * promises never to leave the machine. A monogram tinted by the name is free,
 * offline, and stable: the same domain always gets the same colour.
 *
 * Pass `glyph` for rules that have no name worth abbreviating — a keyword or a
 * wildcard reads better as its symbol than as its first letter.
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

export function TargetIcon({
  value,
  glyph,
  src,
  className,
}: {
  value: string;
  glyph?: ReactNode;
  /**
   * The program's own icon, read off disk. Only applications have one, and
   * only on platforms where we can get at it — everything else falls through
   * to the monogram below.
   */
  src?: string | null;
  className?: string;
}) {
  const hue = hueFor(value);

  if (src) {
    return (
      <img
        src={src}
        alt=""
        // Rendered at its own size inside the tile footprint rather than
        // stretched: icons come back trimmed to their artwork, so forcing a
        // square would distort anything that is not one.
        className={cn("size-9 shrink-0 select-none object-contain", className)}
        draggable={false}
      />
    );
  }

  return (
    <span
      aria-hidden
      className={cn(
        "relative flex size-9 shrink-0 select-none items-center justify-center overflow-hidden",
        "rounded-[0.6rem] font-semibold text-[0.8125rem] ring-1 ring-inset",
        "[&_svg]:size-4",
        className,
      )}
      style={{
        // A lit-from-above tile rather than a flat swatch, matching the cards.
        backgroundImage: `linear-gradient(160deg, oklch(0.38 0.09 ${hue}), oklch(0.28 0.06 ${hue}))`,
        color: `oklch(0.88 0.14 ${hue})`,
        // biome-ignore lint/style/useNamingConvention: CSS custom property
        ["--tw-ring-color" as string]: `oklch(0.58 0.11 ${hue} / 0.4)`,
      }}
    >
      <span
        aria-hidden
        className="absolute inset-x-0 top-0 h-px bg-white/25"
        style={{ maskImage: "linear-gradient(90deg, transparent, #000 30%, #000 70%, transparent)" }}
      />
      {glyph ?? initialFor(value)}
    </span>
  );
}
