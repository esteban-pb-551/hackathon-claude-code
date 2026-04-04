<script setup>
import { computed } from 'vue'
import { useTheme } from './composables/useTheme.js'
import { useSearch } from './composables/useSearch.js'
import { API_URL } from './config.js'
import AppHeader from './components/AppHeader.vue'
import SearchForm from './components/SearchForm.vue'
import SearchResult from './components/SearchResult.vue'
import AppFooter from './components/AppFooter.vue'

console.info('[app] S3 Vectors Search frontend loaded')
console.info('[app] Vue %s | Build: %s', __VUE_OPTIONS_API__ !== undefined ? '3.x' : 'unknown', import.meta.env.MODE)

const { isDark, toggle: toggleTheme } = useTheme()
const { isLoading, elapsedDisplay, result, search, cancel } = useSearch()

const formLocked = computed(() => !isLoading.value && result.value !== null && result.value.type === 'success')

function onSearch({ indexName, query, filter }) {
  console.info('[app] Search submitted:', { indexName, query, filter: filter || '(none)' })
  search(indexName, query, filter)
}

function onReset() {
  result.value = null
}
</script>

<template>
  <div class="app-layout">
    <AppHeader :is-dark="isDark" @toggle-theme="toggleTheme" />

    <SearchForm
      :is-loading="isLoading"
      :elapsed-display="elapsedDisplay"
      :locked="formLocked"
      @search="onSearch"
      @cancel="cancel"
      @reset="onReset"
    />

    <Transition name="fade-slide">
      <SearchResult v-if="result" :result="result" />
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
