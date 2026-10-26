import { QueryClient } from "@tanstack/react-query";

export function createQueryClient() {
  return new QueryClient({
    defaultOptions: {
      queries: {
        // Commands use local IPC (or the loopback development server). Windows'
        // internet status must not pause them indefinitely before they even run.
        networkMode: "always",
        retry: false,
        staleTime: 0,
        refetchInterval: 2000,
        refetchOnWindowFocus: true,
        refetchIntervalInBackground: false,
      },
      mutations: { networkMode: "always", retry: false },
    },
  });
}
