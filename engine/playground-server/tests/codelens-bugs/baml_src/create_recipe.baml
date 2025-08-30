function CreateRecipeVariations(
  description: string,
) -> string {
  client "openai/gpt-4o-mini"
  prompt #"
    tell me about alpha
  "#
}

function CreateRecipeVariations2(
  description: string,
) -> string {
  client "openai/gpt-4o-mini"
  prompt #"
    tell me about beta
  "#
}

function CreateRecipeVariations3(
  description: string,
) -> string {
  client "openai/gpt-4o-mini"
  prompt #"
    tell me about gamma
  "#
}

test create_test1 {
  functions [CreateRecipeVariations, CreateRecipeVariations2, CreateRecipeVariations3]
  args {
    description #"
     description
    "#
  }
}

test create_test2 {
  functions [CreateRecipeVariations]
  args {
    description #"
      Create a recipe for a vegetarian meal.
      Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod
      tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim
      veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea
      commodo consequat. Duis aute irure dolor in reprehenderit in voluptate
      velit esse cillum dolore eu fugiat nulla pariatur. Excepteur sint occaecat
    "#
  }
}

template_string MyPrompt2(description: string) #"
  {{ description }}
          × × × 
          × × × 
          × × × × 
          × × × 
"#

test create_test3 {
  functions [CreateRecipeVariations]
  args {
    description #"
      Create a recipe for a vegetarian meal.
      Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod
      tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim
      veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea
      commodo consequat. Duis aute irure dolor in reprehenderit in voluptate
      velit esse cillum dolore eu fugiat nulla pariatur. Excepteur sint occaecat
    "#
  }
}

test create_test4 {
  functions [CreateRecipeVariations]
  args {
    description #"
      Create a recipe for a vegetarian meal.
      Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod
      tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim
      veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea
      commodo consequat. Duis aute irure dolor in reprehenderit in voluptate
      velit esse cillum dolore eu fugiat nulla pariatur. Excepteur sint occaecat
    "#
  }
}

template_string MyPrompt3(description: string) #"
  {{ description }}
          × × × 
          × × × 
          × × × × 
          × × × 
"#

test create_test5 {
  functions [CreateRecipeVariations]
  args {
    description #"
      Create a recipe for a vegetarian meal.
    "#
  }
}

test create_test6 {
  functions [CreateRecipeVariations]
  args {
    description #"
      Create a recipe for a vegetarian meal.
    "#
  }
}