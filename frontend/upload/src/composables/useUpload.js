import { ref, computed } from 'vue'
import { API_URL } from '../config.js'

export function useUpload() {
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

  async function upload(indexName, filename, filter, fileContent) {
    isLoading.value = true
    result.value = null
    startTimer()

    try {
      // Base64 encode the file content
      const base64Content = btoa(
        new Uint8Array(fileContent).reduce((data, byte) => data + String.fromCharCode(byte), '')
      )

      const body = {
        index_name: indexName,
        filename,
        content: base64Content
      }
      if (filter) body.filter = filter

      const res = await fetch(API_URL, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(body)
      })

      const data = await res.json()
      stopTimer()
      const elapsed = ((Date.now() - startTime) / 1000).toFixed(2)

      if (res.ok) {
        result.value = {
          type: 'success',
          label: 'Uploaded',
          content: 'File uploaded successfully!',
          meta: `${res.status} OK in ${elapsed}s`,
          elapsed,
          request: { indexName, filename, filter: data.filter || 'none' }
        }
      } else if (res.status === 409) {
        result.value = {
          type: 'error',
          label: 'Duplicate',
          content: 'This file has already been uploaded to this index.',
          meta: '409 Conflict',
          elapsed,
          request: { indexName, filename, filter: filter || '' }
        }
      } else {
        result.value = {
          type: 'error',
          label: `Error ${res.status}`,
          content: data.error || JSON.stringify(data, null, 2),
          meta: `${res.status} response`,
          elapsed,
          request: { indexName, filename, filter: filter || '' }
        }
      }
    } catch (err) {
      stopTimer()
      const elapsed = ((Date.now() - startTime) / 1000).toFixed(2)
      result.value = {
        type: 'error',
        label: 'Network Error',
        content: err.message || 'Failed to reach the API.',
        meta: 'Request failed',
        elapsed,
        request: { indexName, filename, filter: filter || '' }
      }
    } finally {
      isLoading.value = false
    }
  }

  return { isLoading, elapsedDisplay, result, upload }
}
