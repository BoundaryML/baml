// Scraped on a private port; no sampling or allocation hooks on GET /.
const http = require('node:http');
async function renderMetrics(sampleHeap) {
  const heap = await sampleHeap();
  const m = process.memoryUsage();
  const values = [
    ['process_resident_memory_bytes', 'Resident memory of the complete serving process, including native allocations.', m.rss],
    ['nodejs_heap_size_used_bytes', 'V8 heap bytes currently used; includes unreachable objects awaiting GC.', m.heapUsed],
    ['nodejs_heap_size_total_bytes', 'V8 heap bytes currently allocated.', m.heapTotal],
    ['nodejs_external_memory_bytes', 'Native memory accounted to JavaScript objects by V8; not all native allocations.', m.external],
    ['nodejs_arraybuffer_memory_bytes', 'ArrayBuffer and Buffer memory; a subset of external memory.', m.arrayBuffers],
    ['process_start_time_seconds', 'Serving process start time in Unix seconds.', Date.now() / 1000 - process.uptime()],
    ['baml_heap_total_object_slots', 'BAML compile-time plus runtime object slots, not heap bytes or live objects.', heap.total_objects],
    ['baml_heap_compile_time_object_slots', 'Permanent BAML compile-time object slots.', heap.compile_time_objects],
    ['baml_heap_runtime_object_slots', 'BAML runtime object slots including unused reservations and uncollected objects.', heap.runtime_objects],
    ['baml_heap_active_handles', 'Registered BAML heap handles acting as GC roots.', heap.active_handles],
    ['baml_heap_tlab_chunks', 'Current BAML nursery allocation reservations; may decrease after GC.', heap.tlab_chunks],
  ];
  return values.map(([name, help, value]) => `# HELP ${name} ${help}\n# TYPE ${name} gauge\n${name} ${value}\n`).join('');
}
function startMetricsServer(sampleHeap) {
  const server = http.createServer(async (req, res) => {
    if (req.method !== 'GET' || req.url !== '/metrics') {
      res.writeHead(404); res.end(); return;
    }
    let body;
    try {
      body = await renderMetrics(sampleHeap);
    } catch (error) {
      console.error('Heap metrics scrape failed:', error);
      res.writeHead(503); res.end(); return;
    }
    res.writeHead(200, {'Content-Type': 'text/plain; version=0.0.4; charset=utf-8', 'Content-Length': Buffer.byteLength(body), 'Cache-Control': 'no-store'});
    res.end(body);
  });
  server.keepAliveTimeout = 60000;
  server.headersTimeout = 65000;
  server.listen(9091, '0.0.0.0');
  return server;
}
module.exports = {renderMetrics, startMetricsServer};
