class Recipe {
    ingredients map<string, Quantity>
}

class Quantity {
    amount int | float
    unit string?
}

function AaaSamOutputFormat(recipe: string) -> Recipe {
  client GPT35
  prompt #"
    Return this value back to me: {{recipe}}

    {{ctx.output_format(map_style='angle')}}
  "#
}

test MyOutput {
    functions [AaaSamOutputFormat]
    args {
        recipe #"
            Here's a simple recipe for beef stew:
Ingredients:

2 lbs beef chuck, cut into 1-inch cubes
2 tbsp vegetable oil
1 onion, diced
3 carrots, sliced
2 celery stalks, chopped
2 potatoes, cubed
3 cloves garlic, minced
4 cups beef broth
1 can (14.5 oz) diced tomatoes
1 tbsp Worcestershire sauce
1 tsp dried thyme
1 bay leaf
Salt and pepper to taste

Instructions:

Season beef with salt and pepper. Heat oil in a large pot over medium-high heat. Brown the beef in batches, then set aside.
In the same pot, sauté onion, carrots, and celery until softened, about 5 minutes.
Add garlic and cook for another minute.
Return beef to the pot. Add broth, tomatoes, Worcestershire sauce, thyme, and bay leaf.
Bring to a boil, then reduce heat and simmer covered for 1 hour.
Add potatoes and continue simmering for 30-45 minutes, until beef and potatoes are tender.
Remove bay leaf, adjust seasoning if needed, and serve hot.

Would you like any additional information or variations on this recipe?
        "#
    }
}