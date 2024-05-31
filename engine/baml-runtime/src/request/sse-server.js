const http = require('http')

const port = 7331

http
  .createServer((req, res) => {
    if (req.url === '/events') {
      console.log('recv')

      res.writeHead(200, {
        'Content-Type': 'text/event-stream',
        'Cache-Control': 'no-cache',
        Connection: 'keep-alive',

        'Access-Control-Allow-Origin': '*', // Allows all domains to access this server
        'Access-Control-Allow-Headers': 'Origin, X-Requested-With, Content-Type, Accept',
      })
      console.log('tx headers')

      let count = 0

      const sendEvent = setInterval(() => {
        const data = `data: ${count}\n\n`
        res.write(data)
        console.log(`tx count=${count}`)

        if (count++ >= 5) {
          clearInterval(sendEvent)
          res.end()
        }
      }, 1000)

      req.on('close', () => {
        console.log('connection closed')
        clearInterval(sendEvent)
        res.end()
      })
    } else {
      res.writeHead(404)
      res.end()
    }
  })
  .listen(port, '0.0.0.0', () => console.log(`Server running on http://localhost:${port}`))
