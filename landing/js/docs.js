;(function () {
  'use strict'

  var DESKTOP_QUERY = '(min-width: 60rem)'

  var toc = document.querySelector('[data-toc]')
  if (!toc) return
  var details = toc.querySelector('details')
  var links = Array.prototype.slice.call(toc.querySelectorAll('a[href^="#"]'))
  var isDesktop = typeof matchMedia === 'function' ? matchMedia(DESKTOP_QUERY) : null

  function syncDetails() {
    if (details && isDesktop) details.open = isDesktop.matches
  }

  function markCurrent(id) {
    links.forEach(function (link) {
      if (link.getAttribute('href') === '#' + id) link.setAttribute('aria-current', 'true')
      else link.removeAttribute('aria-current')
    })
  }

  function initScrollSpy() {
    if (!('IntersectionObserver' in window)) return
    var visible = {}
    var sections = links
      .map(function (link) {
        return document.getElementById(link.getAttribute('href').slice(1))
      })
      .filter(Boolean)

    var observer = new IntersectionObserver(
      function (entries) {
        entries.forEach(function (entry) {
          visible[entry.target.id] = entry.isIntersecting
        })
        var current = sections.filter(function (section) {
          return visible[section.id]
        })[0]
        if (current) markCurrent(current.id)
      },
      { rootMargin: '-20% 0px -70% 0px' },
    )
    sections.forEach(function (section) {
      observer.observe(section)
    })
  }

  links.forEach(function (link) {
    link.addEventListener('click', function () {
      if (details && isDesktop && !isDesktop.matches) details.open = false
    })
  })

  if (isDesktop) {
    syncDetails()
    if (isDesktop.addEventListener) isDesktop.addEventListener('change', syncDetails)
  }
  initScrollSpy()
})()
