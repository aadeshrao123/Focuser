import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "@/assets/tailwind.css";
import { Welcome } from "./Welcome";

const root = document.getElementById("root");
if (root) {
  createRoot(root).render(
    <StrictMode>
      <Welcome />
    </StrictMode>,
  );
}
