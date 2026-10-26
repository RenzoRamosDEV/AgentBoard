;(function () {
  'use strict'

  var THEME_KEY = 'agentboard-site-theme'
  var COUNTER_DURATION_MS = 1200

  var prefersReducedMotion =
    typeof matchMedia === 'function' && matchMedia('(prefers-reduced-motion: reduce)').matches
  var canObserve = 'IntersectionObserver' in window

  function forEach(selector, callback) {
    Array.prototype.forEach.call(document.querySelectorAll(selector), callback)
  }

  function storeTheme(theme) {
    try {
      localStorage.setItem(THEME_KEY, theme)
    } catch (e) {}
  }

  function currentTheme() {
    return document.documentElement.classList.contains('dark') ? 'dark' : 'light'
  }

  function applyTheme(theme) {
    var root = document.documentElement
    root.classList.toggle('dark', theme === 'dark')
    root.style.colorScheme = theme
  }

  function initThemeToggle() {
    var button = document.querySelector('[data-theme-toggle]')
    if (!button) return

    function sync() {
      var dark = currentTheme() === 'dark'
      button.setAttribute('aria-pressed', String(dark))
      button.setAttribute('aria-label', dark ? 'Cambiar a tema claro' : 'Cambiar a tema oscuro')
    }

    button.addEventListener('click', function () {
      var next = currentTheme() === 'dark' ? 'light' : 'dark'
      applyTheme(next)
      storeTheme(next)
      sync()
    })
    sync()
  }

  function onFirstSight(selector, threshold, callback) {
    var observer = new IntersectionObserver(
      function (entries) {
        entries.forEach(function (entry) {
          if (!entry.isIntersecting) return
          observer.unobserve(entry.target)
          callback(entry.target)
        })
      },
      { threshold: threshold },
    )
    forEach(selector, function (element) {
      observer.observe(element)
    })
  }

  function reveal(element) {
    element.classList.add('is-visible')
  }

  function initReveal() {
    if (prefersReducedMotion || !canObserve) {
      forEach('.reveal', reveal)
      return
    }
    onFirstSight('.reveal', 0.15, reveal)
  }

  function easeOutCubic(progress) {
    return 1 - Math.pow(1 - progress, 3)
  }

  function countUp(element) {
    var target = Number(element.textContent)
    if (!isFinite(target) || target <= 0) return
    var startedAt = null

    function frame(now) {
      if (startedAt === null) startedAt = now
      var progress = Math.min((now - startedAt) / COUNTER_DURATION_MS, 1)
      element.textContent = String(Math.round(target * easeOutCubic(progress)))
      if (progress < 1) requestAnimationFrame(frame)
    }

    element.textContent = '0'
    requestAnimationFrame(frame)
  }

  function initCounters() {
    if (prefersReducedMotion || !canObserve) return
    onFirstSight('[data-counter]', 0.6, countUp)
  }

  initThemeToggle()
  initReveal()
  initCounters()
})()
