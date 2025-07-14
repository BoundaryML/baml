// BAML supports multi-modal inputs including videos!

// How to call this BAML function in python or Typescript: https://docs.boundaryml.com/docs/snippets/supported-types#video
// Note: Video input only works with Google models (Gemini)

function VideoInput(videoInput: video) -> string {
  client Gemini
  prompt #"
    {{ _.role("user") }}

    Describe what happens in this video in 5 words or less.
    
    {{ videoInput }}
  "#
}

test TestFileVideoInput {
  functions [VideoInput]
  args {
    videoInput { 
      file "../../../../../integ-tests/baml_src/sample-5s.mp4"
      media_type "video/mp4"
    }
  }  
} 