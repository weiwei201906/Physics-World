<script setup lang="ts">
withDefaults(defineProps<{ lines?: number }>(), { lines: 3 })
</script>

<template>
  <div class="skeleton-block" aria-busy="true" aria-live="polite">
    <span class="skeleton skeleton--title" />
    <span v-for="n in lines" :key="n" class="skeleton" :class="{ 'skeleton--short': n === lines }" />
  </div>
</template>

<style scoped>
.skeleton-block {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  padding: var(--space-5) 0;
}
</style>
