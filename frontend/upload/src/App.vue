<script setup>
import { computed } from 'vue'
import { useTheme } from './composables/useTheme.js'
import { useUpload } from './composables/useUpload.js'
import AppHeader from './components/AppHeader.vue'
import UploadForm from './components/UploadForm.vue'
import UploadResult from './components/UploadResult.vue'
import AppFooter from './components/AppFooter.vue'

const { isDark, toggle: toggleTheme } = useTheme()
const { isLoading, elapsedDisplay, result, upload } = useUpload()

const formLocked = computed(() => !isLoading.value && result.value !== null && result.value.type === 'success')

function onUpload({ indexName, filename, filter, content }) {
  upload(indexName, filename, filter, content)
}

function onReset() {
  result.value = null
}
</script>

<template>
  <div class="app-layout">
    <AppHeader :is-dark="isDark" @toggle-theme="toggleTheme" />

    <UploadForm
      :is-loading="isLoading"
      :elapsed-display="elapsedDisplay"
      :locked="formLocked"
      @upload="onUpload"
      @reset="onReset"
    />

    <Transition name="fade-slide">
      <UploadResult v-if="result" :result="result" />
    </Transition>

    <AppFooter />
  </div>
</template>

<style scoped>
.app-layout {
  max-width: 780px;
  margin: 0 auto;
  padding: 48px 24px 80px;
}

@media (max-width: 600px) {
  .app-layout {
    padding: 24px 16px 60px;
  }
}
</style>
