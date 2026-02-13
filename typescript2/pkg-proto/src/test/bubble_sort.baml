function BubbleSort(arr: int[]) -> int[] {
  client GPT4oMini
  prompt #"
    Sort this array in ascending order using the bubble sort algorithm:
    {{ arr }}

    Return ONLY the sorted array as a JSON array of integers, nothing else.
  "#
}

client<llm> GPT4oMini {
  provider openai
  options {
    model "gpt-4o-mini"
    api_key env.OPENAI_API_KEY
  }
}
