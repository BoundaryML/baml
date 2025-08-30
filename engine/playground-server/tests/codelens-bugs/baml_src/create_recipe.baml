
template_string MyPrompt2(description: string) #"
  {{ description }}
        × × × 
        × × × 
        × × × × 
        × × × 
        × × × 
        × × × 
        × × × × 
        × × × 
        × × × 
        × × × 
        × × × × 
        × × × 
        × × × 
        × × × 
        × × × × 
        × × × 
        × × × 
        × × × 
        × × × × 
        × × × 
        × × × 
        × × × 
        × × × × 
        × × × 
"#
function PredictAlpha(
  description: string,
) -> string {
  client "openai/gpt-4o-mini"
  prompt #" predict alpha "#
}

function TellMeAboutBeta(
  description: string,
) -> string {
  client "openai/gpt-4o-mini"
  prompt #" tell me about beta "#
}

function DoGammaThings(
  description: string,
) -> string {
  client "openai/gpt-4o-mini"
  prompt #" do gamma things "#
}

test create_test1 {
  functions [PredictAlpha, TellMeAboutBeta, DoGammaThings]
  args {
    description #"
     description
    "#
  }
}

test create_test2 {
  functions [PredictAlpha]
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

test create_test3 {
  functions [PredictAlpha]
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
  functions [PredictAlpha]
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
  functions [PredictAlpha]
  args {
    description #"
      Create a recipe for a vegetarian meal.
    "#
  }
}

test create_test6 {
  functions [PredictAlpha]
  args {
    description #"
      Create a recipe for a vegetarian meal.
    "#
  }
}