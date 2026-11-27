<script lang="ts">
  // Small line-icon set (24px grid, 1.8 stroke), drawn for Sweeplark.
  let { name, size = 20 }: { name: string; size?: number } = $props();
  const paths: Record<string, string> = {
    home: "M3 11.5 12 4l9 7.5M5.5 9.5V20h13V9.5M10 20v-5.5h4V20",
    sparkle: "M12 3v4M12 17v4M3 12h4M17 12h4M6 6l2.5 2.5M15.5 15.5 18 18M18 6l-2.5 2.5M8.5 15.5 6 18",
    broom: "M14 4l6 6M12.5 5.5l6 6-4 4-6-6zM8.5 9.5 3 15c-.8.8 1.5 4.8 2.5 5 1-.2 3-1 4.5-2.5l2-2",
    rocket: "M5 15c-1.5 1.5-2 5-2 5s3.5-.5 5-2M9 12l3 3M14.5 4.5C17 3 20.5 3.5 20.5 3.5S21 7 19.5 9.5L13 16l-5-5z",
    pulse: "M3 12h4l2.5-6 4 12 2.5-6H21",
    chat: "M4 5h16v11H9l-5 4z",
    inbox: "M4 13l2.5-8h11L20 13M4 13v6h16v-6M4 13h4.5l1.5 2.5h4L15.5 13H20",
    history: "M3.5 12a8.5 8.5 0 1 0 2.5-6M3 4v4h4M12 8v4.5l3 2",
    settings: "M12 15.2a3.2 3.2 0 1 0 0-6.4 3.2 3.2 0 0 0 0 6.4zM19.4 13.5l1.6 1.2-2 3.4-1.9-.7a7 7 0 0 1-2 1.2L14.8 21h-3.9l-.3-2.4a7 7 0 0 1-2-1.2l-1.9.7-2-3.4 1.6-1.2a7 7 0 0 1 0-2.4L4.7 9.9l2-3.4 1.9.7a7 7 0 0 1 2-1.2L10.9 3h3.9l.3 2.4a7 7 0 0 1 2 1.2l1.9-.7 2 3.4-1.6 1.2a7 7 0 0 1 0 2.4z",
    check: "M5 12.5l4.5 4.5L19 7.5",
    x: "M6 6l12 12M18 6 6 18",
    alert: "M12 4 2.5 20h19zM12 10v4.5M12 17.5v.2",
    info: "M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18zM12 11v5.5M12 7.8v.2",
    shield: "M12 3 5 6v5.5c0 4.4 3 7.8 7 9.5 4-1.7 7-5.1 7-9.5V6z",
    cpu: "M7 7h10v10H7zM10 3v4M14 3v4M10 17v4M14 17v4M3 10h4M3 14h4M17 10h4M17 14h4",
    ram: "M3 8h18v8H3zM7 8v8M11 8v8M15 8v8M5 16v3M19 16v3",
    disk: "M4 5h16v14H4zM4 14h16M16 16.5h.2",
    battery: "M3 8h15v8H3zM20 10.5v3M6 11v2",
    leaf: "M5 19c0-8 5-13 15-14-1 10-6 15-14 15M5 19l7-7",
    restart: "M20 12a8 8 0 1 1-2.3-5.6M20 4v4.5h-4.5",
    undo: "M9 7 4 12l5 5M4 12h10a6 6 0 0 1 0 12h-2",
    send: "M4 12 20 4l-6 16-3-7z",
    chevron: "M9 6l6 6-6 6",
    down: "M6 9l6 6 6-6",
    bird: "M15.5 6.5c1.4-.9 3-.4 3.7.9.8 2-.1 4.4-2.1 6.1-2.3 2-5.4 2.3-7.6 2.6-2.3.4-4.5 1.6-6.8 3.5 1.4-2.8 3.3-4.5 5.6-5.5 2.4-1.1 3.9-2.7 4.6-4.6M18.8 7.2l2.7-.1-2.3 1.9",
    lock: "M6 11h12v9H6zM8.5 11V8a3.5 3.5 0 0 1 7 0v3",
    download: "M12 4v11M7 10.5l5 5 5-5M5 20h14",
    trash: "M5 7h14M10 7V4.5h4V7M7 7l1 13h8l1-13",
    moon: "M20 14.5A8 8 0 0 1 9.5 4 8 8 0 1 0 20 14.5z",
    sun: "M12 16a4 4 0 1 0 0-8 4 4 0 0 0 0 8zM12 2v2.5M12 19.5V22M2 12h2.5M19.5 12H22M4.9 4.9l1.8 1.8M17.3 17.3l1.8 1.8M4.9 19.1l1.8-1.8M17.3 6.7l1.8-1.8",
    file: "M6 3h8l4 4v14H6zM14 3v4h4",
  };
</script>

<svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8"
  stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
  <path d={paths[name] ?? paths.info} />
</svg>
