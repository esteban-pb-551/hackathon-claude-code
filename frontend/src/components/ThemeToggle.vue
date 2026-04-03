<script setup>
defineProps({
  isDark: { type: Boolean, required: true }
})

defineEmits(['toggle'])
</script>

<template>
  <div class="theme-toggle">
    <span class="toggle-icon">{{ isDark ? '\u263E' : '\u2600' }}</span>
    <div
      class="toggle-track"
      :class="{ active: isDark }"
      role="switch"
      :aria-checked="isDark"
      tabindex="0"
      @click="$emit('toggle')"
      @keydown.enter="$emit('toggle')"
      @keydown.space.prevent="$emit('toggle')"
    >
      <div class="toggle-knob" />
    </div>
    <span class="toggle-label">{{ isDark ? 'Dark' : 'Light' }}</span>
  </div>
</template>

<style scoped>
.theme-toggle {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-shrink: 0;
}

.toggle-icon {
  font-size: 14px;
  line-height: 1;
}

.toggle-label {
  font-size: 12px;
  color: var(--text-tertiary);
  font-family: var(--font-mono);
  user-select: none;
}

.toggle-track {
  position: relative;
  width: 44px;
  height: 24px;
  background: var(--toggle-bg);
  border-radius: 12px;
  cursor: pointer;
  transition: background var(--transition);
}

.toggle-track:focus-visible {
  outline: 2px solid var(--border-focus);
  outline-offset: 2px;
}

.toggle-knob {
  position: absolute;
  top: 3px;
  left: 3px;
  width: 18px;
  height: 18px;
  background: var(--toggle-knob);
  border-radius: 50%;
  transition: transform var(--transition);
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.2);
}

.toggle-track.active .toggle-knob {
  transform: translateX(20px);
}
</style>
