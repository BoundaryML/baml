// A cooking step that can contain sub-steps
class CookingStep {
  instruction string
  duration_minutes int?
  temperature int?
  technique string  // "chop", "sauté", "bake", "mix", etc.
  ingredients string[]
  sub_steps CookingStep[]  // Recursive: steps can have sub-steps
  tips string[]
}

// Ingredient with possible substitutions
class Ingredient {
  name string
  amount float
  unit string
  category string  // "protein", "vegetable", "spice", "dairy", etc.
  optional bool
  substitutions Ingredient[]  // Recursive: substitutes can have their own substitutes
}

// Recipe structure
class Recipe {
  name string
  cuisine string
  difficulty string  // "easy", "medium", "hard"
  ingredients Ingredient[]
  steps CookingStep[]
  prep_time_minutes int
  cook_time_minutes int
  servings int
  tags string[]
  nutrition NutritionInfo?
}

class NutritionInfo {
  calories int
  protein_g float
  carbs_g float
  fat_g float
  fiber_g float?
  sodium_mg float?
}

// A meal plan that can contain other meal plans (weekly contains daily, etc.)
class MealPlan {
  name string
  duration string  // "day", "week", "month"
  meals map<string, Recipe | MealPlan>  // Union: can be a recipe or another meal plan
  shopping_list Ingredient[]
  notes string?
}

// Recipe variations and relationships
class RecipeFamily {
  base_recipe Recipe
  variations RecipeVariation[]
  related_families RecipeFamily[]  // Recursive: recipe families can be related
}

class RecipeVariation {
  name string
  changes IngredientChange[]
  step_modifications StepModification[]
  reason string  // "vegetarian", "gluten-free", "budget-friendly", etc.
}

class IngredientChange {
  original Ingredient
  replacement Ingredient?  // Optional: might just remove
  reason string
}

class StepModification {
  original_step_index int
  new_step CookingStep?  // Optional: might just remove the step
  reason string
}

// Analysis request
class RecipeAnalysisRequest {
  user_preferences UserPreferences
  available_ingredients Ingredient[]
  recipes_to_analyze Recipe[]?
  analysis_type string  // "suggest_similar", "create_variations", "meal_planning"
}

class UserPreferences {
  dietary_restrictions string[]  // "vegetarian", "nut-free", "low-sodium", etc.
  favorite_cuisines string[]
  skill_level string
  time_available_minutes int?
  servings_needed int
  equipment_available string[]
  disliked_ingredients string[]
}

// Analysis result with recursive suggestions
class RecipeAnalysisResult {
  suggestions RecipeSuggestion[]
  meal_plans MealPlan[]
  insights RecipeInsight[]
}

class RecipeSuggestion {
  recipe Recipe
  match_score float
  reasoning string
  modifications_needed IngredientChange[]
  similar_recipes RecipeSuggestion[]  // Recursive: suggestions can have related suggestions
}

class RecipeInsight {
  type string  // "tip", "warning", "optimization"
  description string
  affected_recipes string[]
  related_insights RecipeInsight[]  // Recursive: insights can be related
}

// Main function to analyze and suggest recipes
function AnalyzeRecipes(request: RecipeAnalysisRequest) -> RecipeAnalysisResult {
  client "openai/gpt-4o-mini"
  prompt #"
    Analyze recipes based on user preferences and available ingredients.
    
    ## User Preferences
    - Dietary restrictions: {{ request.user_preferences.dietary_restrictions }}
    - Favorite cuisines: {{ request.user_preferences.favorite_cuisines }}
    - Skill level: {{ request.user_preferences.skill_level }}
    - Time available: {{ request.user_preferences.time_available_minutes }} minutes
    - Servings needed: {{ request.user_preferences.servings_needed }}
    
    ## Available Ingredients
    {{ request.available_ingredients }}
    
    {% if request.recipes_to_analyze %}
    ## Recipes to Analyze
    {{ request.recipes_to_analyze }}
    {% endif %}
    
    ## Analysis Type: {{ request.analysis_type }}
    
    Please:
    1. Suggest recipes that match the user's preferences
    2. Consider ingredient substitutions where needed
    3. Group similar recipes together
    4. Create meal plan options if requested
    5. Provide helpful cooking insights
    6. For each recipe, explain why it's a good match
    7. Suggest variations for dietary restrictions
    
    {{ ctx.output_format }}
  "#
}


// Test case
test AnalyzeRecipes {
  functions [AnalyzeRecipes]
  args {
    request {
      user_preferences {
        dietary_restrictions ["vegetarian", "nut-free"]
        favorite_cuisines ["Italian", "Mexican"]
        skill_level "intermediate"
        time_available_minutes 45
        servings_needed 4
        equipment_available ["oven", "stovetop", "blender"]
        disliked_ingredients ["mushrooms", "olives"]
      }
      available_ingredients [
        {
          name "pasta"
          amount 1
          unit "pound"
          category "grain"
          optional false
          substitutions []
        },
        {
          name "tomatoes"
          amount 4
          unit "medium"
          category "vegetable"
          optional false
          substitutions [
            {
              name "tomato sauce"
              amount 2
              unit "cups"
              category "sauce"
              optional false
              substitutions []
            }
          ]
        },
        {
          name "cheese"
          amount 200
          unit "grams"
          category "dairy"
          optional false
          substitutions []
        }
      ]
      analysis_type "suggest_similar"
    }
  }
}