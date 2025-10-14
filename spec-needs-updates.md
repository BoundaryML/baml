# Configuring Timeouts

Timeouts help you build resilient applications by preventing requests from hanging indefinitely. BAML provides granular timeout controls at multiple stages of the request lifecycle.

## Why Use Timeouts?

Without timeouts, your application can stall when:

* LLM provider endpoints are unreachable
* Providers accept requests but take too long to respond
* Network connections stall mid-stream
* Long-running requests exceed your application's latency requirements

Timeouts let you fail fast and either retry or fallback to alternative clients.

## Quick Start

Add timeouts to any client by specifying timeout values in the `http` block within `options`:

```baml
client<llm> MyClient {
  provider openai
  options {
    model "gpt-4"
    api_key env.OPENAI_API_KEY

    // Set timeouts (all values in milliseconds)
    http {
      connect_timeout_ms 5000      // 5 seconds to connect
      request_timeout_ms 30000     // 30 seconds total
    }
  }
}
```

## Available Timeout Types

BAML supports four types of timeouts for individual requests, plus a fifth timeout type for composite clients (fallback, round-robin):

### `connect_timeout_ms`

Maximum time to establish a connection to the LLM provider.

**When to use:** Detect unreachable endpoints quickly.

```baml
client<llm> MyClient {
  provider openai
  options {
    model "gpt-4"
    api_key env.OPENAI_API_KEY
    http {
      connect_timeout_ms 3000  // Fail if can't connect within 3s
    }
  }
}
```

### `time_to_first_token_timeout_ms`

Maximum time to receive the first token after sending the request.

**When to use:** Detect when the provider accepts your request but takes too long to start generating.

```baml
client<llm> MyClient {
  provider openai
  options {
    model "gpt-4"
    api_key env.OPENAI_API_KEY
    http {
      time_to_first_token_timeout_ms 10000  // First token within 10s
    }
  }
}
```

<Tip>
  This timeout is especially useful for streaming responses where you want to ensure the LLM starts responding quickly, even if the full response takes longer.
</Tip>

### `idle_timeout_ms`

Maximum time between receiving data chunks during streaming.

**When to use:** Detect stalled connections where the provider stops sending data mid-response.

```baml
client<llm> MyClient {
  provider openai
  options {
    model "gpt-4"
    api_key env.OPENAI_API_KEY
    http {
      idle_timeout_ms 15000  // No more than 15s between chunks
    }
  }
}
```

### `request_timeout_ms`

Maximum total time for the entire request-response cycle.

**When to use:** Ensure requests complete within your application's latency requirements.

```baml
client<llm> MyClient {
  provider openai
  options {
    model "gpt-4"
    api_key env.OPENAI_API_KEY
    http {
      request_timeout_ms 60000  // Complete within 60s total
    }
  }
}
```

### `total_timeout_ms` (Composite Clients Only)

Maximum time for the entire fallback/round-robin strategy, including all attempts.

**When to use:** Set an overall deadline for your BAML function call.

```baml
client<llm> ResilientClient {
  provider fallback
  options {
    strategy [Primary, Backup, LastResort]
    http {
      total_timeout_ms 120000    // 2 minutes for all attempts combined
    }
  }
}
```

## Timeouts with Retry Policies

Each retry attempt gets the full timeout duration:

```baml
retry_policy Aggressive {
  max_retries 3
  strategy {
    type exponential_backoff
  }
}

client<llm> MyClient {
  provider openai
  retry_policy Aggressive
  options {
    model "gpt-4"
    api_key env.OPENAI_API_KEY
    http {
      request_timeout_ms 30000  // 30s per attempt (applies to each retry)
    }
  }
}
```

If the first attempt times out at 30 seconds, the retry mechanism kicks in and the next attempt gets a fresh 30-second timeout.

**Total time:** Up to 4 attempts Ã— 30s + retry delays = \~2+ minutes

## Timeouts with Fallback Clients

When using fallback clients, each underlying client uses its own timeout settings, while the fallback client can set an overall `total_timeout_ms` to limit the entire chain.

```baml
client<llm> FastClient {
  provider openai
  options {
    model "gpt-3.5-turbo"
    api_key env.OPENAI_API_KEY
    http {
      request_timeout_ms 20000  // Fast client: 20s
    }
  }
}

client<llm> SlowClient {
  provider openai
  options {
    model "gpt-4"
    api_key env.OPENAI_API_KEY
    http {
      request_timeout_ms 60000  // Slower client: 60s
    }
  }
}

client<llm> MyFallback {
  provider fallback
  options {
    strategy [FastClient, SlowClient]
    http {
      total_timeout_ms 120000    // 2 minutes for entire fallback chain
    }
  }
}
```

**Effective timeouts:**

* `FastClient`: Uses its own timeout settings (20s request timeout)
* `SlowClient`: Uses its own timeout settings (60s request timeout)
* Total execution time: Limited to 120 seconds across all attempts by the fallback client's `total_timeout_ms`

<Tip>
  The `total_timeout_ms` provides an upper bound regardless of individual client timeouts. If the fallback chain exhausts 120 seconds, no further clients are attempted. Low-level timeouts like `connect_timeout_ms`, `time_to_first_token_timeout_ms`, and `idle_timeout_ms` should be defined on the individual clients, not on the fallback client itself.
</Tip>

## Runtime Timeout Overrides

Override timeouts at runtime using the [Client Registry](/guide/baml-advanced/llm-client-registry):

## Handling Timeout Errors

Timeout errors are a subclass of `BamlClientError` called `BamlTimeoutError`. You can catch them specifically:

<CodeGroup>
  ```python Python
  from baml_client import b
  from baml_py.errors import BamlTimeoutError, BamlClientError

  try:
      result = await b.ExtractData(input)
  except BamlTimeoutError as e:
      # Handle timeout specifically
      print(f"Request timed out: {e.message}")
      print(f"Timeout type: {e.timeout_type}")
      print(f"Configured: {e.configured_value_ms}ms, Elapsed: {e.elapsed_ms}ms")
  except BamlClientError as e:
      # Handle other client errors
      print(f"Client error: {e.message}")
  ```

  ```typescript TypeScript
  import { b } from './baml_client'
  import { BamlTimeoutError } from '@boundaryml/baml'

  try {
    const result = await b.ExtractData(input)
  } catch (e) {
    if (e instanceof BamlTimeoutError) {
      // Handle timeout specifically
      console.log(`Request timed out: ${e.message}`)
      console.log(`Timeout type: ${e.timeout_type}`)
      console.log(`Configured: ${e.configured_value_ms}ms, Elapsed: ${e.elapsed_ms}ms`)
    } else {
      // Handle other errors
      console.log(`Error: ${e}`)
    }
  }
  ```

  ```ruby Ruby
  begin
    result = b.extract_data(input)
  rescue Baml::TimeoutError => e
    # Handle timeout specifically
    puts "Request timed out: #{e.message}"
    puts "Timeout type: #{e.timeout_type}"
    puts "Configured: #{e.configured_value_ms}ms, Elapsed: #{e.elapsed_ms}ms"
  rescue Baml::ClientError => e
    # Handle other client errors
    puts "Client error: #{e.message}"
  end
  ```
</CodeGroup>

For more on error handling, see [Error Handling](/guide/baml-basics/error-handling).

## Recommended Production Timeouts

For most production applications, we recommend starting with:

```baml
client<llm> ProductionClient {
  provider openai
  options {
    model "gpt-4"
    api_key env.OPENAI_API_KEY

    http {
      connect_timeout_ms 10000                // 10s to connect
      time_to_first_token_timeout_ms 30000    // 30s to first token
      idle_timeout_ms 2000                    // 2s between chunks
      request_timeout_ms 300000               // 5 minutes total
    }
  }
}
```

For fallback clients with stricter requirements:

```baml
client<llm> FallbackClient {
  provider fallback
  options {
    strategy [Primary, Secondary, Tertiary]

    http {
      total_timeout_ms 600000                 // 10 min overall
    }
  }
}
```

## Tips and Best Practices

### Start Conservative, Then Optimize

Begin with generous timeouts and monitor your application's performance. Tighten timeouts gradually based on real-world data.

### Different Timeouts for Different Models

Faster models can use stricter timeouts:

```baml
client<llm> FastTurbo {
  provider openai
  options {
    model "gpt-3.5-turbo"
    api_key env.OPENAI_API_KEY
    http {
      request_timeout_ms 15000  // Turbo is fast
    }
  }
}

client<llm> SlowButSmart {
  provider openai
  options {
    model "gpt-4"
    api_key env.OPENAI_API_KEY
    http {
      request_timeout_ms 60000  // GPT-4 needs more time
    }
  }
}
```

### Use `total_timeout_ms` for User-Facing Features

When building user-facing features, use `total_timeout_ms` to guarantee response times:

```baml
client<llm> ChatbotClient {
  provider fallback
  options {
    strategy [FastModel, SlowModel]
    http {
      total_timeout_ms 5000  // Must respond within 5s for good UX
    }
  }
}
```

### Monitor Timeout Rates

Track how often timeouts occur using [BAML Studio](/guide/boundary-cloud/observability/tracking-usage) or your own observability tools. High timeout rates indicate you should either:

* Increase timeout values
* Use faster models
* Optimize your prompts
* Add more fallback clients

## Timeouts vs Abort Controllers

Timeouts and [abort controllers](/guide/baml-basics/abort-signal) serve different purposes:

* **Timeouts:** Automatic, configuration-based time limits
* **Abort controllers:** Manual, user-initiated cancellation

Use timeouts for resilience and SLAs. Use abort controllers when users explicitly cancel operations.

You can use both together:

```typescript
const controller = new AbortController()

// User clicks "cancel" button
button.onclick = () => controller.abort()

try {
  const result = await b.ExtractData(input, {
    abortController: controller
    // Client still has its configured timeouts
  })
} catch (e) {
  if (e instanceof BamlAbortError) {
    console.log('User cancelled')
  } else if (e instanceof BamlTimeoutError) {
    console.log('Request timed out')
  }
}
```
