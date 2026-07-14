# Deployment shapes and transports

An LLM function and task remain stable whether execution happens in-process,
through HTTP, over JSONL/RPC, or in another service.

## Application server boundary

```baml
function handle_resolve(request: ResolveHttpRequest) -> ResolveHttpResponse {
  let result = ResolveTicket(
    request.ticket,
    $provider = provider_for(request.tenant_id),
  )
  ResolveHttpResponse { resolution: result }
}
```

The server owns authentication, tenant routing, and persistence. The provider
adapter owns vendor authentication and wire format.

## Custom transport adapter

JSONL subprocesses, JSON-RPC daemons, HTTP agents, and language generators can
normalize into the same semantic provider or harness interface:

```text
Task<T>
  -> adapter renders protocol request
  -> transport exchanges frames
  -> adapter emits events / tool calls / terminal T
```

Transport framing is private to the adapter. Application code should not branch
on whether Claude Code uses JSONL and another harness uses RPC.

## Deployment policy is ordinary code

Webhook routes, cron schedules, queues, and autoscaling are host concerns. They
call exported BAML functions or drivers; they are not provider capabilities.

## Related design and scenarios

- Scenarios 26 transports, 35 deployment shapes, 41 harness deployment

