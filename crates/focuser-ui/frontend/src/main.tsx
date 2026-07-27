import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { createHashRouter, RouterProvider } from "react-router-dom";
import { AppLayout } from "@/app-layout";
import "@/index.css";
import { Apps } from "@/routes/apps";
import { BlockLists } from "@/routes/block-lists";
import { Placeholder } from "@/routes/placeholder";
import { Schedule } from "@/routes/schedule";
import { Websites } from "@/routes/websites";

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      // Local IPC, not a network call — a failure is real, so surface it.
      retry: false,

      // State also changes from the blocker loop, the CLI and the extension.
      // The engine re-reads the DB every ~3s; polling keeps the window in step.
      staleTime: 0,
      refetchInterval: 2000,
      refetchOnWindowFocus: true,
      refetchIntervalInBackground: false,
    },
  },
});

// Hash routing: the app is served from the filesystem inside the webview,
// where history-based routing has no server to resolve deep links against.
const router = createHashRouter([
  {
    path: "/",
    element: <AppLayout />,
    children: [
      { index: true, element: <Placeholder title="Dashboard" /> },
      { path: "block-lists", element: <BlockLists /> },
      { path: "websites", element: <Websites /> },
      { path: "apps", element: <Apps /> },
      { path: "schedule", element: <Schedule /> },
      { path: "statistics", element: <Placeholder title="Statistics" /> },
      { path: "settings", element: <Placeholder title="Settings" /> },
    ],
  },
]);

const rootEl = document.getElementById("root");
if (!rootEl) throw new Error("#root missing from index.html");

createRoot(rootEl).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <RouterProvider router={router} />
    </QueryClientProvider>
  </StrictMode>,
);
