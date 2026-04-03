<script setup>
defineProps({
  result: { type: Object, required: true }
})
</script>

<template>
  <div class="result-card" :class="result.type">
    <div class="result-header">
      <div class="result-status">
        <span class="status-dot" />
        {{ result.label }}
      </div>
      <div class="result-meta">{{ result.meta }}</div>
    </div>

    <div class="result-body">
      <div class="result-content">{{ result.content }}</div>
    </div>

    <div class="request-summary">
      <span class="summary-tag">
        <strong>index:</strong> {{ result.request.indexName }}
      </span>
      <span class="summary-tag">
        <strong>query:</strong> {{ result.request.query }}
      </span>
      <span v-if="result.request.filter" class="summary-tag">
        <strong>filter:</strong> {{ result.request.filter }}
      </span>
      <span class="summary-tag">
        <strong>elapsed:</strong> {{ result.elapsed }}s
      </span>
    </div>
  </div>
</template>

<style scoped>
.result-card {
  background: var(--bg-surface);
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
  overflow: hidden;
  box-shadow: var(--shadow-md);
  transition: background var(--transition), border-color var(--transition);
}

.result-card.error {
  border-color: var(--error-border);
}

.result-card.success {
  border-color: var(--success-border);
}

.result-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 14px 20px;
  border-bottom: 1px solid var(--border);
  background: var(--bg-surface-hover);
}

.result-status {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 12px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.6px;
}

.result-card.error .result-status {
  color: var(--error-text);
}

.result-card.success .result-status {
  color: var(--success-text);
}

.status-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
}

.result-card.error .status-dot {
  background: var(--error-text);
}

.result-card.success .status-dot {
  background: var(--success-text);
}

.result-meta {
  font-size: 12px;
  color: var(--text-tertiary);
  font-family: var(--font-mono);
}

.result-body {
  padding: 20px;
}

.result-content {
  font-size: 14px;
  line-height: 1.8;
  white-space: pre-wrap;
  word-break: break-word;
}

.result-card.error .result-content {
  color: var(--error-text);
}

.request-summary {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  padding: 12px 20px;
  border-top: 1px solid var(--border);
  background: var(--bg-surface-hover);
}

.summary-tag {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  font-size: 11px;
  font-family: var(--font-mono);
  color: var(--text-tertiary);
  background: var(--bg-code);
  padding: 3px 8px;
  border-radius: 4px;
}

.summary-tag strong {
  color: var(--text-secondary);
  font-weight: 600;
}
</style>
