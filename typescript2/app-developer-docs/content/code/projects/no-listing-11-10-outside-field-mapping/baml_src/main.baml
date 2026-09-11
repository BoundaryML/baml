// docs:meta
// expect:
//   status: failure
//   diagnostics:
//     - code: E0126
//       messageContains: can only be implemented in the class body
// docs:endmeta
interface Destination { destination: string }
class WebhookNotifier { url: string, }
implements Destination for WebhookNotifier { destination as url }
