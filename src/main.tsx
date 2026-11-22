import React from "react";
import ReactDOM from "react-dom/client";
import { createHashRouter, RouterProvider } from "react-router-dom";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import "@/i18n";
import "@/index.css";
import App from "@/App";

if (import.meta.env.VITE_E2E === "true") void import("@wdio/tauri-plugin");
import { api } from "@/lib/api";
window.addEventListener("error", (event) => {
  const error = event.error instanceof Error ? event.error : null;
  const details = error?.stack
    ? `${event.message}\n${error.stack}`
    : event.message || String(event.error);
  void api.frontendError("uncaught_error", details);
});
window.addEventListener("unhandledrejection", (event) => {
  const reason = event.reason;
  const details =
    reason instanceof Error && reason.stack ? `${reason.message}\n${reason.stack}` : String(reason);
  void api.frontendError("unhandled_rejection", details);
});
const queryClient = new QueryClient({
  defaultOptions: {
    queries: { retry: 1, refetchOnWindowFocus: false, staleTime: 5000 },
  },
});
ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <QueryClientProvider client={queryClient}>
      <RouterProvider router={createHashRouter([{ path: "*", element: <App /> }])} />
    </QueryClientProvider>
  </React.StrictMode>,
);
