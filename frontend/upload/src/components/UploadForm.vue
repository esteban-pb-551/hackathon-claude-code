<script setup>
import { reactive, ref } from 'vue'

const props = defineProps({
  isLoading: { type: Boolean, default: false },
  elapsedDisplay: { type: String, default: '0.0' },
  locked: { type: Boolean, default: false }
})

const emit = defineEmits(['upload', 'reset'])

const form = reactive({
  indexName: '',
  filter: ''
})

const selectedFile = ref(null)
const fileInputRef = ref(null)

const errors = reactive({
  indexName: '',
  file: ''
})

let indexNameTimer = null
let fileTimer = null

function clearError(field) {
  errors[field] = ''
}

function showError(field, message) {
  errors[field] = message
  const timer = setTimeout(() => {
    errors[field] = ''
  }, 3000)
  if (field === 'indexName') {
    clearTimeout(indexNameTimer)
    indexNameTimer = timer
  } else {
    clearTimeout(fileTimer)
    fileTimer = timer
  }
}

function onFileChange(event) {
  const file = event.target.files[0]
  if (!file) {
    selectedFile.value = null
    return
  }
  if (!file.name.endsWith('.txt')) {
    showError('file', 'Only .txt files are allowed')
    selectedFile.value = null
    event.target.value = ''
    return
  }
  clearError('file')
  selectedFile.value = file
}

async function submit() {
  const indexName = form.indexName.trim()
  const filter = form.filter.trim()

  let hasError = false
  if (!indexName) {
    showError('indexName', 'Index Name is required')
    hasError = true
  }
  if (!selectedFile.value) {
    showError('file', 'Please select a .txt file')
    hasError = true
  }
  if (hasError) return

  const arrayBuffer = await selectedFile.value.arrayBuffer()
  emit('upload', {
    indexName,
    filename: selectedFile.value.name,
    filter,
    content: arrayBuffer
  })
}

function reset() {
  form.filter = ''
  selectedFile.value = null
  if (fileInputRef.value) fileInputRef.value.value = ''
  emit('reset')
}
</script>

<template>
  <form class="upload-card" @submit.prevent="submit">
    <div class="form-grid">
      <div class="form-group">
        <label class="form-label" for="indexName">
          Index Name <span class="required">*</span>
        </label>
        <input
          id="indexName"
          v-model="form.indexName"
          class="form-input"
          :class="{ 'input-error': errors.indexName }"
          type="text"
          placeholder="movies"
          autocomplete="off"
          spellcheck="false"
          :disabled="locked || isLoading"
          @input="clearError('indexName')"
        >
        <Transition name="error-fade">
          <span v-if="errors.indexName" class="field-error">{{ errors.indexName }}</span>
        </Transition>
        <span v-if="!errors.indexName" class="form-hint">S3 prefix used as index name</span>
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
          :disabled="locked || isLoading"
        >
        <span class="form-hint">Filterable metadata value</span>
      </div>

      <div class="form-group full-width">
        <label class="form-label" for="file">
          File <span class="required">*</span>
        </label>
        <div class="file-input-wrapper" :class="{ 'input-error': errors.file }">
          <input
            id="file"
            ref="fileInputRef"
            type="file"
            accept=".txt"
            class="file-input"
            :disabled="locked || isLoading"
            @change="onFileChange"
          >
          <div class="file-display">
            <svg class="file-icon" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
              <path d="M9 1H4a1 1 0 0 0-1 1v12a1 1 0 0 0 1 1h8a1 1 0 0 0 1-1V5L9 1Z" />
              <path d="M9 1v4h4" />
            </svg>
            <span v-if="selectedFile" class="file-name">{{ selectedFile.name }}</span>
            <span v-else class="file-placeholder">Choose a .txt file</span>
            <span v-if="selectedFile" class="file-size">{{ (selectedFile.size / 1024).toFixed(1) }} KB</span>
          </div>
        </div>
        <Transition name="error-fade">
          <span v-if="errors.file" class="field-error">{{ errors.file }}</span>
        </Transition>
        <span v-if="!errors.file" class="form-hint">Only .txt files are accepted</span>
      </div>
    </div>

    <div class="form-actions">
      <template v-if="locked">
        <button
          type="button"
          class="btn-new"
          @click="reset"
        >
          Upload Another
        </button>
      </template>

      <template v-else>
        <button
          type="submit"
          class="btn-upload"
          :disabled="isLoading"
        >
          <svg
            class="btn-icon"
            viewBox="0 0 16 16"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
          >
            <path d="M8 12V3" />
            <path d="M4 7l4-4 4 4" />
            <path d="M2 14h12" />
          </svg>
          {{ isLoading ? 'Uploading...' : 'Upload' }}
        </button>

        <div v-if="isLoading" class="loading-bar">
          <div class="progress-track">
            <div class="progress-fill" />
          </div>
          <span>{{ elapsedDisplay }}s</span>
        </div>
      </template>
    </div>
  </form>
</template>

<style scoped>
.upload-card {
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

.form-input {
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

.form-input:focus {
  outline: none;
  border-color: var(--border-focus);
  box-shadow: 0 0 0 3px rgba(74, 111, 165, 0.12);
}

.form-input::placeholder {
  color: var(--text-tertiary);
}

.form-hint {
  font-size: 11px;
  color: var(--text-tertiary);
  margin-top: 2px;
}

.input-error {
  border-color: var(--error-text) !important;
  box-shadow: 0 0 0 3px rgba(239, 68, 68, 0.1);
}

.field-error {
  font-size: 11px;
  color: var(--error-text);
  margin-top: 2px;
  font-weight: 500;
}

.error-fade-enter-active {
  transition: opacity 0.2s ease, transform 0.2s ease;
}

.error-fade-leave-active {
  transition: opacity 0.4s ease, transform 0.4s ease;
}

.error-fade-enter-from {
  opacity: 0;
  transform: translateY(-4px);
}

.error-fade-leave-to {
  opacity: 0;
}

/* -- File Input -- */
.file-input-wrapper {
  position: relative;
  border: 1px dashed var(--border);
  border-radius: var(--radius-sm);
  transition: border-color var(--transition), box-shadow var(--transition);
  cursor: pointer;
}

.file-input-wrapper:hover {
  border-color: var(--border-focus);
}

.file-input {
  position: absolute;
  inset: 0;
  opacity: 0;
  cursor: pointer;
}

.file-display {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 12px 14px;
}

.file-icon {
  width: 20px;
  height: 20px;
  color: var(--text-tertiary);
  flex-shrink: 0;
}

.file-name {
  font-size: 14px;
  font-family: var(--font-mono);
  color: var(--text-primary);
}

.file-placeholder {
  font-size: 14px;
  color: var(--text-tertiary);
}

.file-size {
  margin-left: auto;
  font-size: 12px;
  font-family: var(--font-mono);
  color: var(--text-tertiary);
}

/* -- Actions -- */
.form-actions {
  display: flex;
  align-items: center;
  gap: 16px;
}

.btn-upload {
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

.btn-upload:hover:not(:disabled) {
  background: var(--accent-hover);
}

.btn-upload:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.btn-new {
  padding: 10px 24px;
  background: transparent;
  color: var(--accent);
  border: 1px solid var(--accent);
  border-radius: var(--radius-sm);
  font-size: 14px;
  font-weight: 600;
  font-family: var(--font-sans);
  cursor: pointer;
  transition: background var(--transition), color var(--transition);
}

.btn-new:hover {
  background: var(--accent);
  color: var(--accent-text);
}

.form-input:disabled {
  opacity: 0.6;
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
  .upload-card {
    padding: 20px;
  }
  .form-grid {
    grid-template-columns: 1fr;
  }
}
</style>
