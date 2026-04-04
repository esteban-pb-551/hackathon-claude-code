import { ref, onMounted } from 'vue'

export function useTheme() {
  const isDark = ref(true)

  function apply(dark) {
    const theme = dark ? 'dark' : 'light'
    document.documentElement.setAttribute('data-theme', theme)
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
      isDark.value = window.matchMedia('(prefers-color-scheme: dark)').matches
    }
    apply(isDark.value)
  })

  return { isDark, toggle }
}
