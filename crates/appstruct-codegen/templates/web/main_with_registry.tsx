import { StrictMode } from "react";
import { QueryClientProvider } from "@tanstack/react-query";
import { createRoot } from "react-dom/client";
import "./styles.css";
import { registry } from "../../../app/web/registry";
import { App } from "./app/App";
import { queryClient } from "./query";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <App registry={registry} />
    </QueryClientProvider>
  </StrictMode>,
);
