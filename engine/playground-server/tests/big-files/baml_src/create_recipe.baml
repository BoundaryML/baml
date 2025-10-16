// Function to create recipe variations
function CreateRecipeVariations(
  description: string,
) -> RecipeFamily {
  client "openai/gpt-4o-mini"
  prompt #"
    Create a recipe based on the given description and multiple variations
    
    For each variation goal:
    1. Identify necessary ingredient substitutions
    2. Modify cooking steps if needed
    3. Maintain the essence of the original dish
    4. Explain the changes and their benefits
    5. Consider cascading changes (if you change one thing, what else needs to change?)
    
    Create a family of related recipes with clear relationships between them.
    
    {{ ctx.output_format }}
  "#
}
template_string PromptString(description: string) #"
  {{ description }}
"#

template_string PromptString2(description: string) #"
  {% if documentcontent|length <= 10000 or ctx.client.provider == "openai" %}
  {{ ctx.output_format }}
  {% endif %}

  Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor
  incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis
  nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat.
  Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu
  fugiat nulla pariatur. Excepteur sint occaecat cupidatat non proident, sunt in
  culpa qui officia deserunt mollit anim id est laborum
  {{ description }}
  Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor
  incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis
  nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat.
  Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu
  fugiat nulla pariatur. Excepteur sint occaecat cupidatat non proident, sunt in
  culpa qui officia deserunt mollit anim id est laborum
"#

test create_test1 {
  functions [CreateRecipeVariations]
  args {
    description #"
      Create a recipe for a vegetarian meal.

      Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor
      incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis
      nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat.
      Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu
      fugiat nulla pariatur. Excepteur sint occaecat cupidatat non proident, sunt in
      culpa qui officia deserunt mollit anim id est laborum
    "#
  }
}

test create_test2 {
  functions [CreateRecipeVariations]
  args {
    description #"
      Create a recipe for a vegetarian meal.
      Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor
      incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis
      nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat.
      Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu
      fugiat nulla pariatur. Excepteur sint occaecat cupidatat non proident, sunt in
      culpa qui officia deserunt mollit anim id est laborum
    "#
  }
}

test create_test3 {
  functions [CreateRecipeVariations]
  args {
    description #"
      Create a recipe for a vegetarian meal.
      Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor
      incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis
      nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat.
      Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu
      fugiat nulla pariatur. Excepteur sint occaecat cupidatat non proident, sunt in
      culpa qui officia deserunt mollit anim id est laborum
    "#
  }
}

test create_test4 {
  functions [CreateRecipeVariations]
  args {
    description #"
      Create a recipe for a vegetarian meal.
      Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor
      incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis
      nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat.
      Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu
      fugiat nulla pariatur. Excepteur sint occaecat cupidatat non proident, sunt in
      culpa qui officia deserunt mollit anim id est laborum
    "#
  }
}

test create_test5 {
  functions [CreateRecipeVariations]
  args {
    description #"
      Create a recipe for a vegetarian meal.
      Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor
      incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis
      nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat.
      Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu
      fugiat nulla pariatur. Excepteur sint occaecat cupidatat non proident, sunt in
      culpa qui officia deserunt mollit anim id est laborum
    "#
  }
}

test create_test6 {
  functions [CreateRecipeVariations]
  args {
    description #"
      Create a recipe for a vegetarian meal.
      Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor
      incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis
      nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat.
      Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu
      fugiat nulla pariatur. Excepteur sint occaecat cupidatat non proident, sunt in
      culpa qui officia deserunt mollit anim id est laborum
    "#
  }
}