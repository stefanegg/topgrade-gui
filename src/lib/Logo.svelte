<script lang="ts">
  /** The app icon (src-tauri/icons/source/app-icon.svg), cropped to the square. */
  let { size = 28 }: { size?: number } = $props();
</script>

<svg width={size} height={size} viewBox="100 100 824 824" aria-hidden="true">
  <rect x="100" y="100" width="824" height="824" rx="185" fill="#085041" />
  <g fill="none" stroke="#9FE1CB" stroke-width="69" stroke-linecap="round" stroke-linejoin="round">
    <path d="M340 701 L512 546 L684 701" opacity="0.45" />
    <path d="M340 564 L512 409 L684 564" opacity="0.7" />
    <path d="M340 426 L512 272 L684 426" />
  </g>
</svg>
