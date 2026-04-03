import { ref, onMounted } from 'vue'

/**
 * Composable for dark / light theme management.
 * Persists the user's choice in localStorage and respects
 * the system preference on first visit.
 */
export function useTheme() {
  const isDark = ref(true)

  function apply(dark) {
    document.documentElement.setAttribute(
      'data-theme',
      dark ? 'dark' : 'light'
    )
  }

  function toggle() {
    isDark.value = !isDark.value
    apply(isDark.value)
    localStorage.setItem('theme', isDark.value ? 'dark' : 'light')
  }

  onMounted(() => {
    const saved = localStorage.getItem('theme')
    if (saved) {
      isDark.value = saved === 'dark'
    } else {
      isDark.value = window.matchMedia(
        '(prefers-color-scheme: dark)'
      ).matches
    }
    apply(isDark.value)
  })

  return { isDark, toggle }
}
