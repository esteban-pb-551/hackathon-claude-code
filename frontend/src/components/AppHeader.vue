<script setup>
import ThemeToggle from './ThemeToggle.vue'

defineProps({
  isDark: { type: Boolean, required: true }
})

defineEmits(['toggleTheme'])
</script>

<template>
  <header class="header">
    <div class="header-info">
      <h1>S3 Vectors Search</h1>
      <p class="tagline">
        Semantic search powered by S3 Vectors, VoyageAI, and GLM-5
      </p>
      <div class="badges">
        <span class="badge">RAG Pipeline</span>
        <span class="badge">Serverless</span>
        <span class="badge">Rust</span>
      </div>
    </div>

    <ThemeToggle :is-dark="isDark" @toggle="$emit('toggleTheme')" />
  </header>
</template>

<style scoped>
.header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  margin-bottom: 36px;
  padding-bottom: 24px;
  border-bottom: 1px solid var(--border);
}

.header-info h1 {
  font-size: 24px;
  font-weight: 700;
  letter-spacing: -0.4px;
  margin-bottom: 4px;
}

.tagline {
  font-size: 13px;
  color: var(--text-secondary);
  font-family: var(--font-mono);
}

.badges {
  display: flex;
  gap: 6px;
  margin-top: 8px;
}

.badge {
  display: inline-flex;
  align-items: center;
  padding: 3px 8px;
  font-size: 11px;
  font-weight: 600;
  font-family: var(--font-mono);
  background: var(--badge-bg);
  color: var(--badge-text);
  border-radius: 4px;
  letter-spacing: 0.2px;
}

@media (max-width: 600px) {
  .header {
    flex-direction: column;
    gap: 16px;
  }
}
</style>
