function AudioInput(aud: audio) -> string{
  client Gemini
  prompt #"
    {{ _.role("user") }}

   Does this sound like a roar? Yes or no? One word no other characters.
    
    {{ aud }}
  "#   
}  
//        

client GPT4Audio {
  provider openai
  options {
    model "gpt-4o-audio-preview"
  }
}

function AudioInputOpenai(aud: audio, prompt: string) -> string{
  client GPT4Audio
  prompt #"
    {{ _.role("user") }}

   {{ prompt }}
    
    {{ aud }}
  "#   
}  
//        
       
      
     
test TestURLAudioInput{
  functions [AudioInput]
  args {
    aud{ 
      url https://actions.google.com/sounds/v1/emergency/beeper_emergency_call.ogg
    }
  }  
}

test TestURLAudioInputOpenai{
  functions [AudioInputOpenai]
  args {
    aud{ 
      url https://actions.google.com/sounds/v1/emergency/beeper_emergency_call.ogg
    }
    prompt "Does this sound like a roar? Yes or no? One word no other characters."
  }  
}


