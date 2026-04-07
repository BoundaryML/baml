function ExtractContact(text: string) -> ContactInfo {
  client GPT4oMini
  prompt #"
    Extract contact information from the following text.

    Respond in JSON with this exact schema:
    {"name": "string", "email": "string", "phone": "string or null"}

    Text:
    {{ text }}

    JSON:
  "#
}
