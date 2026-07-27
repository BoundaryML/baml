# Images, PDFs, and audio

Media values may appear in LLM function arguments and prompts without being
converted to ad hoc JSON.

## Utilities used

| Type | Meaning |
| --- | --- |
| `image` | Image content or reference |
| `pdf` | PDF document |
| `audio` | Finite audio content |

## Example

```baml
class ClaimEvidence {
  merchant: string?,
  amount: float?,
  notes: string[],
}

function InspectClaim(
  receipt: image,
  statement: pdf,
  explanation: audio,
) -> ClaimEvidence {
  provider: "openai/gpt-5.6-luna"
  prompt: `
    Review the receipt, statement, and recorded explanation.

    Receipt:
    ${receipt}

    Statement:
    ${statement}

    Explanation:
    ${explanation}

    ${ctx.output_format}
  `
}

let evidence = InspectClaim(
  receipt_image,
  bank_statement,
  customer_recording,
)
```

The provider adapter chooses its wire representation for each media part.
The LLM function keeps a provider-independent typed signature.

Finite audio is different from a live microphone. A finite `audio` value has a
known completion boundary; a realtime channel returns a resource.

[Back to media and live sessions](../media-and-live-sessions.md)
