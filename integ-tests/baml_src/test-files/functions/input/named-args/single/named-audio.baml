class Response {
  type string
  reason string
  appropriate_for_video_games bool
  score int @description(#"
    1-5, 1 being best at video game background music and 5 being worst
  "#)
}

function AudioInput(aud: audio) -> Response {
  client Gemini
  prompt #"
    Describe the sound.

    {{ ctx.output_format(prefix="Answer in this schema:\n") }}


    {{ _.role("user") }}
    
    {{ aud }}
  "#
}


test TestURLAudioInput{
  functions [AudioInput]
  args {
    aud{ 
      url https://actions.google.com/sounds/v1/emergency/beeper_emergency_call.ogg
    }
  }  
}


