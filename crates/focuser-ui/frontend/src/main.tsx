import { QueryClientProvider } from "@tanstack/react-query";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { createHashRouter, RouterProvider } from "react-router-dom";
import { AppLayout } from "@/app-layout";
import { Toaster } from "@/components/ui/toast";
import { TooltipProvider } from "@/components/ui/tooltip";
import { createQueryClient } from "@/lib/query-client";
import "@/index.css";
import { Allowances } from "@/routes/allowances";
import { Apps } from "@/routes/apps";
import { BlockLists } from "@/routes/block-lists";
import { Dashboard } from "@/routes/dashboard";
import { Schedule } from "@/routes/schedule";
import { Settings } from "@/routes/settings";
import { Websites } from "@/routes/websites";

const queryClient = createQueryClient();

// Hash routing: the app is served from the filesystem inside the webview,
// where history-based routing has no server to resolve deep links against.
const router = createHashRouter([
  {
    path: "/",
    element: <AppLayout />,
    children: [
      { index: true, element: <Dashboard /> },
      { path: "block-lists", element: <BlockLists /> },
      { path: "websites", element: <Websites /> },
      { path: "apps", element: <Apps /> },
      { path: "schedule", element: <Schedule /> },
      { path: "allowances", element: <Allowances /> },
      // Split out: the charting library is a third of the bundle and most
      // sessions never open this page.
      {
        path: "statistics",
        lazy: async () => ({ Component: (await import("@/routes/statistics")).Statistics }),
      },
      { path: "settings", element: <Settings /> },
    ],
  },
]);

const rootEl = document.getElementById("root");
if (!rootEl) throw new Error("#root missing from index.html");

createRoot(rootEl).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <TooltipProvider>
        <RouterProvider router={router} />
        <Toaster />
      </TooltipProvider>
    </QueryClientProvider>
  </StrictMode>,
);
