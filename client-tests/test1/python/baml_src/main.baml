generator Default {
    language "python"
    project_root "../"
    test_command "infisical run -- poetry run python -m pytest"
    install_command "poetry add baml@latest"
    package_version_command "poetry show baml"
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
    