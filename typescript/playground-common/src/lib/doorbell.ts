import { useEffect } from 'react'

const loadDoorbell = () => {
  ;(window as any).doorbellOptions = {
    id: '14262',
    appKey: '4coKRb0R5AVhGfyJOuDm45GYC7NsD8uQUfe5x2zNHEh2FYTZhQxks1l1d0kyWHsC',
  }
  ;(function (w: any, d, t) {
    var hasLoaded = false
    function l() {
      if (hasLoaded) {
        return
      }
      hasLoaded = true
      ;(window as any).doorbellOptions.windowLoaded = true
      var g = d.createElement(t) as any
      g.id = 'doorbellScript'
      g.type = 'text/javascript'
      g.crossorigin = 'anonymous'
      g.async = true
      g.src = 'https://embed.doorbell.io/button/' + (window as any).doorbellOptions['id'] + '?t=' + new Date().getTime()
      ;(d.getElementsByTagName('head')[0] || d.getElementsByTagName('body')[0]).appendChild(g)
    }
    if (w.attachEvent) {
      w.attachEvent('onload', l)
    } else if (w.addEventListener) {
      w.addEventListener('load', l, false)
    } else {
      l()
    }
    if (d.readyState == 'complete') {
      l()
    }
  })(window, document, 'script')
}

export const useDoorbell = () => {
  useEffect(() => {
    loadDoorbell()
  }, [])
}
