import "./app.css";
import { mount } from "svelte";
import App from "./App.svelte";
import { applyTheme, initialTheme } from "./lib/state.svelte";

// Set the theme before the first paint (no flash). Not saved until the user picks one,
// so a first start keeps following Windows.
applyTheme(initialTheme(), false);

// Desktop app: no browser context menu or page zoom.
document.addEventListener("contextmenu", (e) => {
  if (!(e.target as HTMLElement).closest(".selectable, input, textarea")) e.preventDefault();
});

export default mount(App, { target: document.getElementById("app")! });
