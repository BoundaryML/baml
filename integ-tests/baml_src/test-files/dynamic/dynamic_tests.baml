class DynamicSchema {
  @@dynamic
}

function ExtractEntities(text: string) -> DynamicSchema {
  client Ollama
  prompt #"
    Parse the following text and return a structured representation of the data in the schema below.

    text:
    ---
    {{text}}
    ---

    {{ ctx.output_format }}
  "#
}


test ExtractEntitiesTest {
  functions [ExtractEntities]
  
  type_builder {
        enum Gender {
            MALE
            FEMALE
            OTHER
        }
        enum PreferredContactInfo{
            EMAIL
            PHONE
        }
        class PersonalInfo{
            
            first_name  string?
            last_name  string?
            gender  Gender?
            
        }
        class ContactInfo2 {
            email  string? @description(#"The customer's email address"#)
            phone  string? @description(#"The customer's phone number"#)  
            preferred_contact_method  PreferredContactInfo? @description(#"The customer's preferred method of contact"#)   

        }
        dynamic class DynamicSchema {
            personal_info PersonalInfo
            contact_info ContactInfo2
        }
  }
  
  args {    
       "text" "Agent: Good morning! Thank you for reaching out. I’ll need to collect some basic details to assist you better. Could you please provide your first and last name? Customer: Sure! My name is John Doe. Agent: Thank you, John. May I also ask for your gender? Customer: I'd prefer not to share that at the moment. Agent: No problem at all. Now, for contact purposes, could you share your email address? Customer: Yes, my email is johndo@example.com. Agent: Great! Do you have a phone number where we can reach you? Customer: I’d rather not provide that right now. Agent: That’s completely fine. How would you prefer us to contact you—by email or phone? Customer: Please contact me via Email. Agent: Understood! Lastly, can you share the reason for your call today? Customer: I’m not ready to specify that just yet. Agent: That’s okay, John! I’ve noted everything down. If you need any further assistance, feel free to reach out. Have a great day!"
  } 
   
  
}