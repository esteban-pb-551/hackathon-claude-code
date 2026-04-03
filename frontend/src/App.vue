<script setup>
import { useTheme } from './composables/useTheme.js'
import { useSearch } from './composables/useSearch.js'
import AppHeader from './components/AppHeader.vue'
import SearchForm from './components/SearchForm.vue'
import SearchResult from './components/SearchResult.vue'
import AppFooter from './components/AppFooter.vue'

const { isDark, toggle: toggleTheme } = useTheme()
const { isLoading, elapsedDisplay, result, search } = useSearch()

function onSearch({ indexName, query, filter }) {
  search(indexName, query, filter)
}
</script>

<template>
  <div class="app-layout">
    <AppHeader :is-dark="isDark" @toggle-theme="toggleTheme" />

    <SearchForm
      :is-loading="isLoading"
      :elapsed-display="elapsedDisplay"
      @search="onSearch"
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
