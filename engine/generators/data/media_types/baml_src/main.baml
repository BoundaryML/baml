// Test media types in BAML - image, audio, PDF, video
// Media types can only be used as input parameters, not outputs

class MediaAnalysisResult {
  topics string[]
  analysisText string
}

class MediaArrayAnalysisResult {
  analysisText string
  mediaCount int
}

class MediaMapAnalysisResult {
  analysisText string
  keyCount int
  keys string[]
}

class OptionalMediaAnalysisResult {
  analysisText string
  providedMediaTypes string[]
  missingMediaTypes string[]
}

class MixedMediaAnalysisResult {
  title string
  description string
  hasImage bool
  hasVideo bool
  hasAudio bool
  hasPdf bool
  additionalImageCount int
  metadataKeys string[]
}

// Test functions with media types as input parameters
function TestMediaInput(media: image | audio | pdf | video, textInput: string) -> MediaAnalysisResult {
  client "openai/gpt-4o-mini"
  prompt #"

    Describe what is provided.

    {{ ctx.output_format }}


    {{ _.role('user') }}
    Text: {{ textInput }}
    {{ media }}    
  "#
}

function TestMediaArrayInputs(
  imageArray: image[], 
  textInput: string
) -> MediaArrayAnalysisResult {
  client "openai/gpt-4o-mini"
  prompt #"
    {{ ctx.output_format }}

    {{ _.role('user') }}
    Text: {{ textInput }}
    {{ imageArray }}    
  "#
}
