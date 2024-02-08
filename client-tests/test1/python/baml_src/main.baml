generator Default {
  language python
  pkg_manager poetry
//   python_setup_prefix "infisical run --env=test -- poetry run"
}    
               
enum MessageSender {             
    AI @alias("ai")          
    RESIDENT @alias("resident")   
}             
  
class Message {
    sender MessageSender @alias("sender1") 
    body   string @alias("body1")
}   
  
class Conversation {
    thread Message[] 
}      
 