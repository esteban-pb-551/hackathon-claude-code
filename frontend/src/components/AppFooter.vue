<template>
  <footer class="footer">
    <span>S3 Vectors Semantic Search</span>
    <span class="endpoint">POST /search</span>
  </footer>
</template>

<style scoped>
.footer {
  margin-top: 48px;
  padding-top: 20px;
  border-top: 1px solid var(--border);
  display: flex;
  align-items: center;
  justify-content: space-between;
  font-size: 12px;
  color: var(--text-tertiary);
  font-family: var(--font-mono);
}

@media (max-width: 600px) {
  .footer {
    flex-direction: column;
    gap: 8px;
    text-align: center;
  }
}
</style>
