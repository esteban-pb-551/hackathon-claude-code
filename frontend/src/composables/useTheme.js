import { ref, onMounted } from 'vue'

/**
 * Composable for dark / light theme management.
 * Persists the user's choice in localStorage and respects
 * the system preference on first visit.
 */
export function useTheme() {
  const isDark = ref(true)

  function apply(dark) {
    const theme = dark ? 'dark' : 'light'
    document.documentElement.setAttribute('data-theme', theme)
    console.info('[theme] Applied: %s', theme)
  }

  function toggle() {
    isDark.value = !isDark.value
    apply(isDark.value)
    localStorage.setItem('theme', isDark.value ? 'dark' : 'light')
    console.info('[theme] Toggled to: %s (saved to localStorage)', isDark.value ? 'dark' : 'light')
  }

  onMounted(() => {
    const saved = localStorage.getItem('theme')
    if (saved) {
      isDark.value = saved === 'dark'
      console.info('[theme] Loaded from localStorage: %s', saved)
    } else {
      isDark.value = window.matchMedia(
        '(prefers-color-scheme: dark)'
      ).matches
      console.info('[theme] No saved preference, using system: %s', isDark.value ? 'dark' : 'light')
    }
    apply(isDark.value)
  })

  return { isDark, toggle }
}
