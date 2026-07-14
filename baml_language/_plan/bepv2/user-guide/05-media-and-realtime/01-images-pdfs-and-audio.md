# Images, PDFs, and finite audio

Finite media values are task inputs. They remain structural in the prompt so
each provider adapter can encode them for its own wire protocol.

## Multimodal input

```baml
function InspectDamage(
  photo: image,
  warranty: pdf,
) -> DamageAssessment {
  provider: VisionModel
  prompt: `
    Inspect ${photo} and check coverage in ${warranty}.
    ${ctx.output_format}
  `
}

let assessment = InspectDamage(photo, warranty)
```

A provider that cannot encode one of the media parts returns typed
`Unsupported`; it must not silently remove the attachment.

## Specialized media output

Use the driver matching the operation:

```baml
let image_result = ai.drivers.generate_image(image_task, image_options)
let transcript = ai.drivers.transcribe(audio_task, transcription_options)
let speech = ai.drivers.generate_speech(speech_task, speech_options)
```

Specialized task types are appropriate where the input is not naturally a
prompt-shaped LLM function. The same ownership rule remains: task is intent,
driver is lifecycle, provider is wire protocol.

## Finite versus live

An `audio` value is complete and normally replayable. `AudioStream` is
incremental and may be single-use. A realtime `Channel` is duplex and exposes
an ongoing interactive lifecycle. Do not treat these as synonyms.

## Related design and scenarios

- Scenarios 05 multimodal input, 06 non-text output, 25 voice pipelines

