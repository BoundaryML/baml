retry_policy GPT4oRetry {
  max_retries 2
  strategy {
    type exponential_backoff
  }
}

client<llm> GPT35 {
  provider openai
  retry_policy GPT4oRetry
  options {
    model "gpt-4o-mini"
    api_key env.OPENAI_API_KEY
  }
}

class Tree {
  data int
  children Forest
}

class Forest {
  trees Tree[]
}

class BinaryNode {
  data int
  left BinaryNode?
  right BinaryNode?
}

function FnOutputMutuallyRecursiveClasses(input: BinaryNode) -> Tree {
  client GPT35
  prompt #"
    Given the input binary tree, transform it into a generic tree using the given schema.

    INPUT:
    {{ input }}

    {{ ctx.output_format() }}
  "#
}

test TestOutputMutuallyRecursiveClasses {
  functions [FnOutputMutuallyRecursiveClasses]
  args {
    input {
      data 2
      left {
        data 1
        left null
        right null
      }
      right {
        data 3
        left null
        right null
      }
    }
  }
}
