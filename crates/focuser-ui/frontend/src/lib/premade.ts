/**
 * Curated starter lists, served from `public/premade-lists.json`.
 *
 * Fetched rather than imported so the ~1,200 domains stay out of the JS bundle
 * and can be refreshed without a rebuild.
 */

import { useQuery } from "@tanstack/react-query";
import { z } from "zod";

const category = z.object({
  name: z.string(),
  description: z.string().default(""),
  domains: z.array(z.string()).default([]),
  wildcards: z.array(z.string()).default([]),
});

const file = z.object({
  version: z.number(),
  categories: z.record(z.string(), category),
});

export interface PremadeCategory {
  id: string;
  name: string;
  description: string;
  domains: string[];
  wildcards: string[];
}

export function usePremadeLists() {
  return useQuery({
    queryKey: ["premade-lists"] as const,
    queryFn: async (): Promise<PremadeCategory[]> => {
      const response = await fetch("premade-lists.json");
      if (!response.ok) throw new Error(`Could not load the starter lists (${response.status})`);

      const parsed = file.parse(await response.json());
      return Object.entries(parsed.categories)
        .map(([id, c]) => ({ id, ...c }))
        .sort((a, b) => a.name.localeCompare(b.name));
    },
    // A static file that ships with the app — fetching it once is enough.
    staleTime: Number.POSITIVE_INFINITY,
    refetchInterval: false,
  });
}
