function VideoInputGemini(vid: video) -> string{
  client Gemini
  prompt #"
    {{ _.role("user") }}

    Use 5 words to describe the video.
    
    {{ vid }}
  "#   
}  

function VideoInputVertex(vid: video) -> string{
  client Vertex
  prompt #"
    {{ _.role("user") }}

    Use 5 words to describe the video.
    
    {{ vid }}
  "#   
}  

test TestFileVideoInput{
  functions [VideoInputGemini]
  args {
    vid{ 
      file "../../../../../sample-5s.mp4"
    }
  }  
}

test TestFileVideoInput2{
  functions [VideoInputVertex]
  args {
    vid{ 
      file "../../../../../sample-5s.mp4"
    }
  }  
}


test TestFileVideoInputWithMediaType{
  functions [VideoInputGemini]
  args {
    vid { 
      file "../../../../../sample-5s.mp4"
      media_type "video/mp4"
    }
  }  
}

test TestFileVideoInputWithUrl{
  functions [VideoInputGemini]
  args {
    vid { 
      url "https://youtu.be/dQw4w9WgXcQ?si=aQdfsK0DdcDtCCud"
    }
  }  
}

test TestFileVideoInputWithUrlVertex{
  functions [VideoInputVertex]
  args {
    vid { 
      url "https://youtu.be/dQw4w9WgXcQ?si=aQdfsK0DdcDtCCud"
      media_type "video/mp4"
    }
  }  
}