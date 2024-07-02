function AudioInput(audioInput: audio) -> string{
  client Gemini
  prompt #"
    {{ _.role("user") }}

   Does this sound like a roar? Yes or no? One word no other characters.
    
    {{ audioInput }}
  "#
}


test TestURLAudioInput{
  functions [AudioInput]
  args {
    audioInput { 
      url https://actions.google.com/sounds/v1/emergency/beeper_emergency_call.ogg
    }
  }  
}


