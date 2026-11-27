import "./app.css";
import { mount } from "svelte";
import App from "./App.svelte";

try {
  const t = localStorage.getItem("theme");
  if (t === "light" || t === "dark") document.documentElement.setAttribute("data-theme", t);
} catch { /* ignore */ }

// Desktop app: no browser context menu or page zoom.
document.addEventListener("contextmenu", (e) => {
  if (!(e.target as HTMLElement).closest(".selectable, input, textarea")) e.preventDefault();
});

export default mount(App, { target: document.getElementById("app")! });
