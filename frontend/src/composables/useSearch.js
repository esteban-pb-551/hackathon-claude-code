import { ref, computed, onUnmounted } from 'vue'
import { API_URL } from '../config.js'

/**
 * Composable that encapsulates the search API call,
 * loading state, elapsed-time counter, and result handling.
 */
export function useSearch() {
  const isLoading = ref(false)
  const elapsedMs = ref(0)
  const result = ref(null)

  let timer = null
  let startTime = 0

  const elapsedDisplay = computed(() =>
    (elapsedMs.value / 1000).toFixed(1)
  )

  function startTimer() {
    startTime = Date.now()
    elapsedMs.value = 0
    timer = setInterval(() => {
      elapsedMs.value = Date.now() - startTime
    }, 100)
  }

  function stopTimer() {
    if (timer) {
      clearInterval(timer)
      timer = null
    }
  }

  onUnmounted(() => stopTimer())

  async function search(indexName, query, filter) {
    const body = { index_name: indexName, query }
    if (filter) body.filter = filter

    isLoading.value = true
    result.value = null
    startTimer()

    try {
      const res = await fetch(API_URL, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(body)
      })

      const data = await res.json()
      stopTimer()
      const elapsed = ((Date.now() - startTime) / 1000).toFixed(2)

      if (data.error) {
        result.value = {
          type: 'error',
          label: `Error ${res.status}`,
          content: data.error,
          meta: `${res.status} response`,
          elapsed,
          request: { indexName, query, filter }
        }
      } else if (data.response) {
        result.value = {
          type: 'success',
          label: 'Response',
          content: data.response,
          meta: `200 OK in ${elapsed}s`,
          elapsed,
          request: { indexName, query, filter }
        }
      } else {
        result.value = {
          type: 'error',
          label: 'Unexpected Response',
          content: JSON.stringify(data, null, 2),
          meta: `${res.status} response`,
          elapsed,
          request: { indexName, query, filter }
        }
      }
    } catch (err) {
      stopTimer()
      const elapsed = ((Date.now() - startTime) / 1000).toFixed(2)
      result.value = {
        type: 'error',
        label: 'Network Error',
        content:
          err.message ||
          'Failed to reach the API. Check the endpoint URL and your network connection.',
        meta: 'Request failed',
        elapsed,
        request: { indexName, query, filter }
      }
    } finally {
      isLoading.value = false
    }
  }

  return { isLoading, elapsedDisplay, result, search }
}
