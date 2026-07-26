import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { createHashRouter, RouterProvider } from "react-router-dom";
import { AppLayout } from "@/app-layout";
import "@/index.css";
import { BlockLists } from "@/routes/block-lists";
import { Placeholder } from "@/routes/placeholder";

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      // The backend is a local IPC call, not a network round-trip: retrying
      // buys nothing, and a failure is a real error worth surfacing at once.
      retry: false,
      // Data can change from outside the UI (background blocker loop, the CLI,
      // the browser extension), so treat cached reads as immediately stale.
      staleTime: 0,
      refetchOnWindowFocus: true,
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
      { path: "websites", element: <Placeholder title="Websites" /> },
      { path: "apps", element: <Placeholder title="Applications" /> },
      { path: "schedule", element: <Placeholder title="Schedule" /> },
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
