<script setup>
import { reactive } from 'vue'

const props = defineProps({
  isLoading: { type: Boolean, default: false },
  elapsedDisplay: { type: String, default: '0.0' }
})

const emit = defineEmits(['search'])

const form = reactive({
  indexName: '',
  query: '',
  filter: ''
})

function submit() {
  const indexName = form.indexName.trim()
  const query = form.query.trim()
  const filter = form.filter.trim()
  if (!indexName || !query) return
  emit('search', { indexName, query, filter })
}
</script>

<template>
  <form class="search-card" @submit.prevent="submit">
    <div class="form-grid">
      <div class="form-group">
        <label class="form-label" for="indexName">
          Index Name <span class="required">*</span>
        </label>
        <input
          id="indexName"
          v-model="form.indexName"
          class="form-input"
          type="text"
          placeholder="movies"
          required
          autocomplete="off"
          spellcheck="false"
        >
        <span class="form-hint">S3 prefix used as index name</span>
      </div>

      <div class="form-group">
        <label class="form-label" for="filter">
          Filter <span class="optional-tag">optional</span>
        </label>
        <input
          id="filter"
          v-model="form.filter"
          class="form-input"
          type="text"
          placeholder="scifi"
          autocomplete="off"
          spellcheck="false"
        >
        <span class="form-hint">Filter by metadata value</span>
      </div>

      <div class="form-group full-width">
        <label class="form-label" for="query">
          Query <span class="required">*</span>
        </label>
        <textarea
          id="query"
          v-model="form.query"
          class="form-textarea"
          placeholder="What are the main themes in the movie?"
          rows="3"
          required
          spellcheck="false"
        />
      </div>
    </div>

    <div class="form-actions">
      <button
        type="submit"
        class="btn-search"
        :disabled="isLoading || !form.indexName.trim() || !form.query.trim()"
      >
        <svg
          class="btn-icon"
          viewBox="0 0 16 16"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
        >
          <circle cx="7" cy="7" r="5" />
          <line x1="11" y1="11" x2="14.5" y2="14.5" />
        </svg>
        {{ isLoading ? 'Searching...' : 'Search' }}
      </button>

      <div v-if="isLoading" class="loading-bar">
        <div class="progress-track">
          <div class="progress-fill" />
        </div>
        <span>{{ elapsedDisplay }}s</span>
      </div>
    </div>
  </form>
</template>

<style scoped>
.search-card {
  background: var(--bg-surface);
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
  padding: 28px;
  box-shadow: var(--shadow-sm);
  margin-bottom: 28px;
  transition: background var(--transition), border-color var(--transition),
    box-shadow var(--transition);
}

.form-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 16px;
  margin-bottom: 16px;
}

.form-group {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.form-group.full-width {
  grid-column: 1 / -1;
}

.form-label {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  font-weight: 600;
  color: var(--text-secondary);
  text-transform: uppercase;
  letter-spacing: 0.6px;
}

.form-label .required {
  color: var(--error-text);
  font-weight: 400;
}

.form-label .optional-tag {
  font-size: 10px;
  font-weight: 500;
  text-transform: none;
  letter-spacing: 0;
  color: var(--text-tertiary);
  background: var(--bg-code);
  padding: 1px 6px;
  border-radius: 3px;
}

.form-input,
.form-textarea {
  width: 100%;
  padding: 10px 14px;
  background: var(--bg-input);
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  color: var(--text-primary);
  font-size: 14px;
  font-family: var(--font-mono);
  transition: border-color var(--transition), box-shadow var(--transition),
    background var(--transition);
}

.form-textarea {
  resize: vertical;
  min-height: 88px;
  line-height: 1.5;
}

.form-input:focus,
.form-textarea:focus {
  outline: none;
  border-color: var(--border-focus);
  box-shadow: 0 0 0 3px rgba(74, 111, 165, 0.12);
}

.form-input::placeholder,
.form-textarea::placeholder {
  color: var(--text-tertiary);
}

.form-hint {
  font-size: 11px;
  color: var(--text-tertiary);
  margin-top: 2px;
}

.form-actions {
  display: flex;
  align-items: center;
  gap: 16px;
}

.btn-search {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  padding: 10px 24px;
  background: var(--accent);
  color: var(--accent-text);
  border: none;
  border-radius: var(--radius-sm);
  font-size: 14px;
  font-weight: 600;
  font-family: var(--font-sans);
  cursor: pointer;
  transition: background var(--transition), opacity var(--transition);
}

.btn-search:hover:not(:disabled) {
  background: var(--accent-hover);
}

.btn-search:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.btn-icon {
  display: inline-flex;
  width: 16px;
  height: 16px;
}

.loading-bar {
  display: flex;
  align-items: center;
  gap: 12px;
  font-size: 13px;
  color: var(--text-secondary);
  font-family: var(--font-mono);
}

.progress-track {
  width: 120px;
  height: 4px;
  background: var(--loading-track);
  border-radius: 2px;
  overflow: hidden;
}

.progress-fill {
  height: 100%;
  background: var(--loading-fill);
  border-radius: 2px;
  animation: progress-indeterminate 1.5s ease-in-out infinite;
}

@keyframes progress-indeterminate {
  0% {
    width: 0%;
    margin-left: 0;
  }
  50% {
    width: 60%;
    margin-left: 20%;
  }
  100% {
    width: 0%;
    margin-left: 100%;
  }
}

@media (max-width: 600px) {
  .search-card {
    padding: 20px;
  }
  .form-grid {
    grid-template-columns: 1fr;
  }
}
</style>
