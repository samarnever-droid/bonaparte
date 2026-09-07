<script lang="ts">
  let {
    name,
    size = 16,
    class: className = "",
  }: { name: string; size?: number; class?: string } = $props();
  const icons: Record<string, string[]> = {
    cube: ["M12 2 21 7v10l-9 5-9-5V7l9-5", "M3 7l9 5 9-5M12 12v10"],
    camera: ["M4 8h3l2-3h6l2 3h3v12H4z", "M12 17a4 4 0 1 0 0-8 4 4 0 0 0 0 8"],
    target: ["M12 12m-8 0a8 8 0 1 0 16 0 8 8 0 1 0-16 0", "M12 12m-3 0a3 3 0 1 0 6 0 3 3 0 1 0-6 0", "M12 2v3M12 19v3M2 12h3M19 12h3"],
    turntable: ["M12 12m-3 0a3 3 0 1 0 6 0 3 3 0 1 0-6 0", "M20 12a8 8 0 1 1-2.3-5.6", "M18 3v4h-4"],
    wave: ["M2 10v4M6 6v12M10 3v18M14 7v10M18 5v14M22 10v4"],
    scissors: [
      "M6 8a3 3 0 1 0 0-6 3 3 0 0 0 0 6M6 22a3 3 0 1 0 0-6 3 3 0 0 0 0 6M8 7l13 14M8 17 21 3",
    ],
    headphones: ["M3 14v-3a9 9 0 0 1 18 0v3M3 12h4v9H3zM17 12h4v9h-4z"],
    marker: ["M6 3h12v11l-6 7-6-7z"],
    loop: ["M20 8H6a4 4 0 0 0-4 4v2M16 4l4 4-4 4M4 16h14a4 4 0 0 0 4-4v-2M8 12l-4 4 4 4"],
    plus: ["M12 5v14M5 12h14"],
    x: ["m6 6 12 12M18 6 6 18"],
    check: ["m5 12 4 4L19 6"],
    search: ["M20 20l-5-5", "M17 10a7 7 0 1 1-14 0 7 7 0 0 1 14 0"],
    folder: ["M3 7V5h6l2 2h10v13H3Z"],
    film: ["M3 3h18v18H3zM7 3v18M17 3v18M3 8h4M3 16h4M17 8h4M17 16h4"],
    layers: ["m12 3 10 5-10 5L2 8Z", "m2 12 10 5 10-5M2 16l10 5 10-5"],
    sliders: ["M4 4v16M12 4v16M20 4v16M1 8h6M9 16h6M17 10h6"],
    type: ["M4 6V3h16v3M12 3v18M8 21h8"],
    square: ["M4 4h16v16H4z"],
    circle: ["M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0"],
    adjust: ["M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0M12 3v18M12 6h4M12 9h7M12 12h9M12 15h7M12 18h4"],
    play: ["m8 4 13 8-13 8Z"],
    pause: ["M8 4v16M16 4v16"],
    back: ["M5 5v14M18 5l-10 7 10 7Z"],
    forward: ["M19 5v14M6 5l10 7-10 7Z"],
    undo: ["M3 10h11a7 7 0 0 1 0 14", "m8 5-5 5 5 5"],
    redo: ["M21 10H10a7 7 0 0 0 0 14", "m16 5 5 5-5 5"],
    save: ["M4 3h13l4 4v14H3V3Z", "M7 3v6h10V3M7 21v-8h10v8"],
    upload: ["M12 16V3m-5 5 5-5 5 5M3 16v5h18v-5"],
    download: ["M12 3v13m-5-5 5 5 5-5M3 16v5h18v-5"],
    eye: ["M2 12s4-7 10-7 10 7 10 7-4 7-10 7S2 12 2 12Z", "M15 12a3 3 0 1 1-6 0 3 3 0 0 1 6 0"],
    "eye-off": [
      "m3 3 18 18M9 5.5A12 12 0 0 1 12 5c6 0 10 7 10 7s-1 2-3 4M6 6.5C3.5 9 2 12 2 12s4 7 10 7c2 0 3.5-.5 5-1.5",
    ],
    lock: ["M5 10h14v11H5zM8 10V6a4 4 0 0 1 8 0v4"],
    unlock: ["M5 10h14v11H5zM8 10V6a4 4 0 0 1 8 0"],
    trash: ["M3 6h18M8 6V3h8v3M6 6l1 15h10l1-15M10 10v7M14 10v7"],
    copy: ["M8 8h13v13H8zM16 8V3H3v13h5"],
    up: ["m6 14 6-6 6 6"],
    down: ["m6 10 6 6 6-6"],
    right: ["m9 5 7 7-7 7"],
    left: ["m15 5-7 7 7 7"],
    grid: ["M3 3h7v7H3zM14 3h7v7h-7zM3 14h7v7H3zM14 14h7v7h-7z"],
    pointer: ["m5 3 15 9-7 2-3 7Z"],
    hand: [
      "M8 12V5a2 2 0 0 1 4 0v7V3a2 2 0 0 1 4 0v9-6a2 2 0 0 1 4 0v8c0 5-3 8-7 8-3 0-5-2-7-5l-3-5a2 2 0 0 1 3-2l2 2",
    ],
    maximize: ["M3 9V3h6M15 3h6v6M21 15v6h-6M9 21H3v-6"],
    settings: [
      "M12 8a4 4 0 1 1 0 8 4 4 0 0 1 0-8M9 3h6l1 3 3 1 2 5-2 5-3 1-1 3H9l-1-3-3-1-2-5 2-5 3-1Z",
    ],
    palette: [
      "M12 3a9 9 0 1 0 0 18h2a2 2 0 0 0 1-4 2 2 0 0 1 1-4h2c4 0 4-10-6-10",
      "M7 10h.01M10 6h.01M15 7h.01M6 15h.01",
    ],
    sparkles: ["m12 3 2.5 6.5L21 12l-6.5 2.5L12 21l-2.5-6.5L3 12l6.5-2.5ZM20 2v4M18 4h4"],
    graph: ["M3 3v18h18M5 17C15 17 8 6 20 6"],
    keyframe: ["m12 4 8 8-8 8-8-8Z"],
    link: [
      "m9 15 6-6M7 17l-1 1a4 4 0 0 1-6-6l5-5a4 4 0 0 1 6 0M17 7l1-1a4 4 0 0 1 6 6l-5 5a4 4 0 0 1-6 0",
    ],
    rotate: ["M21 11a9 9 0 1 0-2 7M21 3v8h-8"],
    history: ["M3 11a9 9 0 1 1 1 6M3 3v8h8M12 7v6l4 2"],
    image: ["M3 3h18v18H3zM3 17l5-5 4 4 4-5 5 6M9 7h.01"],
    info: ["M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0M12 11v6M12 7h.01"],
    help: ["M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0M9 9a3 3 0 1 1 4 3c-1 1-1 1-1 3M12 18h.01"],
    more: ["M5 12h.01M12 12h.01M19 12h.01"],
    bolt: ["m13 2-9 12h7l-1 8 10-13h-7Z"],
    terminal: ["m4 5 6 6-6 6M13 18h7"],
  };
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="1.6"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
  class={className}
>
  {#each icons[name] ?? icons.square as d}<path {d} />{/each}
</svg>
