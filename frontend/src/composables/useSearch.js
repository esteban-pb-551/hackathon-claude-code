import { ref, computed, onUnmounted } from 'vue'
import { API_URL } from '../config.js'

const POLL_INTERVAL_MS = 4000
const POLL_TIMEOUT_MS = 5 * 60 * 1000 // 5 minutes safety net

/**
 * Composable that encapsulates the async polling search flow,
 * loading state, elapsed-time counter, and result handling.
 *
 * Flow:
 *   1. POST /search  -> 202 { request_id }
 *   2. Poll GET /search/{request_id} every 2s
 *   3. Resolve when status is "complete" or "error"
 */
export function useSearch() {
  const isLoading = ref(false)
  const elapsedMs = ref(0)
  const result = ref(null)

  let timer = null
  let pollTimer = null
  let pollResolve = null
  let startTime = 0

  const elapsedDisplay = computed(() =>
    (elapsedMs.value / 1000).toFixed(1)
  )

  // --- internal helpers ---

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

  function stopPolling() {
    if (pollTimer) {
      clearInterval(pollTimer)
      pollTimer = null
    }
  }

  function cleanup() {
    stopTimer()
    stopPolling()
  }

  function cancel() {
    if (!isLoading.value) return
    console.info('[search] Cancelled by user')
    cleanup()
    const elapsed = ((Date.now() - startTime) / 1000).toFixed(2)
    result.value = {
      type: 'error',
      label: 'Cancelled',
      content: 'Search cancelled by user.',
      meta: `Cancelled at ${elapsed}s`,
      elapsed,
      request: { indexName: '', query: '', filter: '' }
    }
    isLoading.value = false
    if (pollResolve) {
      pollResolve()
      pollResolve = null
    }
  }

  onUnmounted(() => cleanup())

  // --- main search function ---

  async function search(indexName, query, filter) {
    const body = { index_name: indexName, query }
    if (filter) body.filter = filter

    isLoading.value = true
    result.value = null
    startTimer()

    try {
      // Step 1: POST to kick off the search (expects HTTP 202)
      console.info('[search] POST %s', API_URL, { index_name: indexName, query, filter: filter || '(none)' })
      const postRes = await fetch(API_URL, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(body)
      })

      console.info('[search] POST response: %d %s', postRes.status, postRes.statusText)
      const postData = await postRes.json()
      console.info('[search] POST body:', postData)

      if (!postData.request_id) {
        // Backend did not return a request_id — treat as unexpected response
        console.error('[search] No request_id in response. Status: %d, body:', postRes.status, postData)
        stopTimer()
        const elapsed = ((Date.now() - startTime) / 1000).toFixed(2)
        result.value = {
          type: 'error',
          label: `Error ${postRes.status}`,
          content: postData.error || JSON.stringify(postData, null, 2),
          meta: `${postRes.status} response`,
          elapsed,
          request: { indexName, query, filter }
        }
        isLoading.value = false
        return
      }

      const requestId = postData.request_id
      console.info('[search] Got request_id: %s — starting polling', requestId)

      // Step 2: Poll GET /search/{request_id} every 2 seconds
      const pollUrl = `${API_URL}/${requestId}`
      const pollStart = Date.now()
      let pollCount = 0

      await new Promise((resolve) => {
        pollResolve = resolve
        pollTimer = setInterval(async () => {
          pollCount++

          // Safety-net timeout
          if (Date.now() - pollStart > POLL_TIMEOUT_MS) {
            console.error('[poll] Timeout after %d polls (%ds)', pollCount, ((Date.now() - pollStart) / 1000).toFixed(1))
            stopPolling()
            stopTimer()
            const elapsed = ((Date.now() - startTime) / 1000).toFixed(2)
            result.value = {
              type: 'error',
              label: 'Timeout',
              content: 'The search timed out after 5 minutes of polling.',
              meta: 'Polling timeout',
              elapsed,
              request: { indexName, query, filter }
            }
            isLoading.value = false
            resolve()
            return
          }

          try {
            console.info('[poll] #%d GET %s', pollCount, pollUrl)
            const pollRes = await fetch(pollUrl)
            const pollData = await pollRes.json()
            console.info('[poll] #%d status: %s', pollCount, pollData.status, pollData)

            if (pollData.status === 'complete') {
              console.info('[poll] Complete after %d polls. Response length: %d chars', pollCount, (pollData.response || '').length)
              stopPolling()
              stopTimer()
              const elapsed = ((Date.now() - startTime) / 1000).toFixed(2)
              result.value = {
                type: 'success',
                label: 'Response',
                content: pollData.response,
                meta: `200 OK in ${elapsed}s`,
                elapsed,
                request: { indexName, query, filter }
              }
              isLoading.value = false
              resolve()
            } else if (pollData.status === 'error') {
              console.error('[poll] Backend error after %d polls:', pollCount, pollData.error || pollData)
              stopPolling()
              stopTimer()
              const elapsed = ((Date.now() - startTime) / 1000).toFixed(2)
              result.value = {
                type: 'error',
                label: 'Error',
                content: pollData.error || 'Unknown error from backend.',
                meta: 'Backend error',
                elapsed,
                request: { indexName, query, filter }
              }
              isLoading.value = false
              resolve()
            }
            // status === "pending" → keep polling
          } catch (pollErr) {
            // Network error during polling
            console.error('[poll] Network error on poll #%d:', pollCount, pollErr)
            stopPolling()
            stopTimer()
            const elapsed = ((Date.now() - startTime) / 1000).toFixed(2)
            result.value = {
              type: 'error',
              label: 'Network Error',
              content:
                pollErr.message ||
                'Lost connection while polling for results.',
              meta: 'Polling failed',
              elapsed,
              request: { indexName, query, filter }
            }
            isLoading.value = false
            resolve()
          }
        }, POLL_INTERVAL_MS)
      })
    } catch (err) {
      // Network error on the initial POST
      console.error('[search] POST failed:', err)
      stopTimer()
      stopPolling()
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
      isLoading.value = false
    }
  }

  return { isLoading, elapsedDisplay, result, search, cancel }
}
