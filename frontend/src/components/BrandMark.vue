<script setup lang="ts">
withDefaults(defineProps<{ size?: number }>(), { size: 28 })
</script>

<template>
  <svg
    class="brand-mark"
    :width="size"
    :height="size"
    viewBox="0 0 32 32"
    fill="none"
    aria-hidden="true"
    focusable="false"
  >
    <path d="M3 27 H29" stroke="currentColor" stroke-width="1.5" stroke-linecap="square" />
    <path d="M3 27 V5" stroke="currentColor" stroke-width="1.5" stroke-linecap="square" />
    <path
      d="M4 25 C10 25 10 9 17 8 C23 7.2 26 13 28 20"
      stroke="currentColor"
      stroke-width="1.5"
      stroke-linecap="round"
    />
    <path d="M24.5 18.5 L28 20 L27 16.5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" />
    <circle cx="17" cy="8" r="1.6" fill="currentColor" />
  </svg>
</template>

<style scoped>
.brand-mark {
  display: block;
  color: currentColor;
  flex-shrink: 0;
}
</style>
