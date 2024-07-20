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

const loadSignalZen = () => {
  var _sz = (_sz || {}) as any
  ;(_sz.appId = '03fb7e7f'),
    (function () {
      var e = document.createElement('script')
      ;(e.src = 'https://cdn.signalzen.com/signalzen.js'),
        e.setAttribute('async', 'true'),
        document.documentElement.firstChild?.appendChild(e)
      var t = setInterval(function () {
        const SignalZen = (window as any).SignalZen
        'undefined' != typeof SignalZen && (clearInterval(t), new SignalZen(_sz).load())
      }, 10)
    })()
}

const loadSmallChat = () => {
  const e = document.createElement('script')
  e.src = 'https://embed.small.chat/T03KV1PH19PC07DA5G14HY.js'
  e.setAttribute('async', 'true')
  document.documentElement.firstChild?.appendChild(e)
}

export const useFeedbackWidget = () => {
  useEffect(() => {
    loadDoorbell()
    // loadSignalZen()
    // loadSmallChat()
  }, [])
}
