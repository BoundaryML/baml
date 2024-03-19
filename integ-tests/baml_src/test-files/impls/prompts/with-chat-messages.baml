

impl<llm, PromptTest> claude_chat_with_chat_msgs {
  client Claude
  prompt #"
    {#chat(system)}You are an assistant that always responds in a very excited way with emojis and also outputs this word 4 times after giving a response: {#input}{// test the input vars get replaced even in system prompt//}
    
    {#chat(user)}Tell me a haiku about {#input}
  "#
} 

impl<llm, PromptTest> claude_chat_with_chat_msgs_no_system {
  client Claude
  prompt #"
    You are an assistant that always responds in a very excited way with emojis and also outputs this word 4 times after giving a response: {#input}{// test the input vars get replaced even in system prompt//}
    
    {#chat(user)}Tell me a haiku about {#input}
  "#
} 

impl<llm, PromptTest> openai_chat_with_chat_msgs {
  client GPT35
  prompt #"
    {#chat(system)}You are an assistant that always responds in a very excited way with emojis and also outputs this word 4 times after giving a response: {#input}{// test the input vars get replaced even in system prompt//}

    {#chat(user)}Tell me a haiku about {#input}
  "#
}


impl<llm, PromptTest> openai_chat_with_chat_msgs_no_system {
  client GPT35
  prompt #"
    {#chat(system)}You are an assistant that always responds in a very excited way with emojis and also outputs this word 4 times after giving a response: {#input}{// test the input vars get replaced even in system prompt//}

    {#chat(user)}Tell me a haiku about {#input}
  "#
}
