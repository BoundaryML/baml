import { useEffect } from 'react'

// const loadDoorbell = () => {
//   ;(window as any).doorbellOptions = {
//     id: '14262',
//     appKey: '4coKRb0R5AVhGfyJOuDm45GYC7NsD8uQUfe5x2zNHEh2FYTZhQxks1l1d0kyWHsC',
//   }
//   ;(function (w: any, d, t) {
//     var hasLoaded = false
//     function l() {
//       if (hasLoaded) {
//         return
//       }
//       hasLoaded = true
//       ;(window as any).doorbellOptions.windowLoaded = true
//       var g = d.createElement(t) as any
//       g.id = 'doorbellScript'
//       g.type = 'text/javascript'
//       g.crossorigin = 'anonymous'
//       g.async = true
//       g.src = 'https://embed.doorbell.io/button/' + (window as any).doorbellOptions['id'] + '?t=' + new Date().getTime()
//       ;(d.getElementsByTagName('head')[0] || d.getElementsByTagName('body')[0]).appendChild(g)
//     }
//     if (w.attachEvent) {
//       w.attachEvent('onload', l)
//     } else if (w.addEventListener) {
//       w.addEventListener('load', l, false)
//     } else {
//       l()
//     }
//     if (d.readyState == 'complete') {
//       l()
//     }
//   })(window, document, 'script')
// }

// const loadSignalZen = () => {
//   var _sz = (_sz || {}) as any
//   ;(_sz.appId = '03fb7e7f'),
//     (function () {
//       var e = document.createElement('script')
//       ;(e.src = 'https://cdn.signalzen.com/signalzen.js'),
//         e.setAttribute('async', 'true'),
//         document.documentElement.firstChild?.appendChild(e)
//       var t = setInterval(function () {
//         const SignalZen = (window as any).SignalZen
//         'undefined' != typeof SignalZen && (clearInterval(t), new SignalZen(_sz).load())
//       }, 10)
//     })()
// }

// const loadHubspot = () => {
//   const e = document.createElement('script')
//   e.id = 'hs-script-loader'
//   e.src = 'https://js-na1.hs-scripts.com/46827730.js'
//   e.setAttribute('type', 'text/javascript')
//   e.setAttribute('async', 'true')
//   e.setAttribute('defer', 'true')
//   if (!document.getElementById(e.id)) {
//     document.documentElement.firstChild?.appendChild(e)
//   }
// }

const loadChatwoot = () => {
  ;(function (d, t) {
    var BASE_URL = 'https://app.chatwoot.com'
    var g = d.createElement(t) as any,
      s = d.getElementsByTagName(t)[0] as any
    g.src = BASE_URL + '/packs/js/sdk.js'
    g.defer = true
    g.async = true
    s.parentNode.insertBefore(g, s)
    g.onload = function () {
      ;(window as any).chatwootSDK.run({
        websiteToken: 'M4EXKvdb9NGgxqZzkTZfeFV7',
        baseUrl: BASE_URL,
        position: 'left',
      })
    }
  })(document, 'script')
}

export const useFeedbackWidget = () => {
  useEffect(() => {
    // loadDoorbell()
    // loadSignalZen()
    // loadSmallChat()
    // loadHubspot()
    loadChatwoot()
  }, [])
}
