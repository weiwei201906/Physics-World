<script setup lang="ts">
withDefaults(
  defineProps<{
    name?: 'atom' | 'pendulum' | 'wave' | 'prism' | 'vector'
    size?: number
  }>(),
  { name: 'atom', size: 32 }
)
</script>

<template>
  <svg
    class="glyph"
    :width="size"
    :height="size"
    viewBox="0 0 40 40"
    fill="none"
    aria-hidden="true"
    focusable="false"
  >
    <g v-if="name === 'atom'" stroke="currentColor" stroke-width="1.25">
      <circle cx="20" cy="20" r="5" />
      <ellipse cx="20" cy="20" rx="15" ry="5.5" transform="rotate(30 20 20)" />
      <ellipse cx="20" cy="20" rx="15" ry="5.5" transform="rotate(-30 20 20)" />
      <circle cx="20" cy="20" r="1.5" fill="currentColor" stroke="none" />
    </g>
    <g v-else-if="name === 'pendulum'" stroke="currentColor" stroke-width="1.25" stroke-linecap="round">
      <path d="M4 6 H36" />
      <path d="M20 6 L28 30" />
      <circle cx="28.7" cy="32" r="3.2" />
      <path d="M20 6 L12 30" stroke-dasharray="2 3" />
      <path d="M8 36 H32" />
    </g>
    <g v-else-if="name === 'wave'" stroke="currentColor" stroke-width="1.25" stroke-linecap="round">
      <path d="M3 22 C7 10 11 10 15 22 C19 34 23 34 27 22 C31 10 35 10 37 18" />
      <path d="M3 30 H37" stroke-dasharray="2 3" />
      <path d="M3 14 H37" stroke-dasharray="2 3" />
    </g>
    <g v-else-if="name === 'prism'" stroke="currentColor" stroke-width="1.25" stroke-linejoin="round">
      <path d="M20 7 L33 31 H7 Z" />
      <path d="M3 18 H15" />
      <path d="M25 26 H37" />
      <path d="M27 22 H39" />
      <path d="M29 30 H39" />
    </g>
    <g v-else stroke="currentColor" stroke-width="1.25" stroke-linecap="round">
      <path d="M5 35 V5 H35" />
      <path d="M5 35 L33 9" />
      <path d="M26 9 H33 V16" />
      <path d="M9 20 H15" />
      <path d="M20 20 H26" />
      <path d="M9 20 V26" />
    </g>
  </svg>
</template>

<style scoped>
.glyph {
  display: block;
  color: currentColor;
  flex-shrink: 0;
}
</style>
