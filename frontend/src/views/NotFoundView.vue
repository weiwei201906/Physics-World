<script setup lang="ts">
import { RouterLink } from 'vue-router'
</script>

<template>
  <div class="not-found">
    <p class="code">404</p>
    <h1>This page drifted off the grid.</h1>
    <p class="desc">The page you are looking for does not exist or has been moved.</p>
    <RouterLink to="/" class="btn btn-primary">Back to dashboard</RouterLink>
  </div>
</template>

<style scoped>
.not-found {
  min-height: 100vh;
  display: flex;
  flex-direction: column;
  justify-content: center;
  align-items: center;
  text-align: center;
  padding: var(--space-6);
  gap: var(--space-3);
}

.code {
  font-family: var(--font-display);
  font-size: 5rem;
  font-weight: 700;
  color: var(--color-primary-700);
  margin: 0;
  line-height: 1;
}

.desc {
  color: var(--color-text-muted);
  max-width: 360px;
}
</style>
