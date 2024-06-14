// app/providers.tsx
'use client'
import posthog from 'posthog-js'
import { PostHogProvider } from 'posthog-js/react'
import { useEffect } from 'react'

if (typeof window !== 'undefined' && process.env.NODE_ENV === 'production') {
  posthog.init(process.env.NEXT_PUBLIC_POSTHOG_KEY!, {
    api_host: process.env.NEXT_PUBLIC_POSTHOG_HOST,
    capture_pageview: false, // Disable automatic pageview capture, as we capture manually
  })
}

const RB2BId = process.env.NEXT_PUBLIC_RB2B_ID ?? ''

export function PHProvider({ children }: { children: React.ReactNode }) {
  return <PostHogProvider client={posthog}>{children}</PostHogProvider>
}

export function RB2BElement() {
  useEffect(() => {
    // Directly adding your script content here
    function f() {
      // @ts-ignore
      var reb2b = (window.reb2b = window.reb2b || [])
      if (reb2b.invoked) return
      reb2b.invoked = true
      reb2b.methods = ['identify', 'collect']
      reb2b.factory = function (method: string) {
        return function () {
          var args = Array.prototype.slice.call(arguments)
          args.unshift(method)
          reb2b.push(args)
          return reb2b
        }
      }
      for (var i = 0; i < reb2b.methods.length; i++) {
        var key = reb2b.methods[i]
        reb2b[key] = reb2b.factory(key)
      }
      reb2b.load = function (key: string) {
        var script = document.createElement('script')
        script.type = 'text/javascript'
        script.async = true
        script.src = 'https://s3-us-west-2.amazonaws.com/b2bjsstore/b/' + key + '/reb2b.js.gz'
        var first = document.getElementsByTagName('script')[0]
        first.parentNode?.insertBefore(script, first)
      }
      reb2b.SNIPPET_VERSION = '1.0.1'
      if (RB2BId) {
        reb2b.load(RB2BId)
      }
    }

    f()
  }, [])

  return <></>
}
