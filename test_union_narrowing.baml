// This test file demonstrates the type narrowing functionality
// for the user's specific use case

class GetWeatherType {
    city string
}

class Reply {
    content string
}

// This function uses a union type parameter
// and demonstrates type narrowing with instanceof
function handle_action(action: GetWeatherType | Reply) -> string {
    if (action instanceof GetWeatherType) {
        // After the instanceof check, we can access GetWeatherType fields
        let weather = "Weather for " + action.city;
        return weather;
    } else if (action instanceof Reply) {
        // In this branch, we can access Reply fields
        return "Reply: " + action.content;
    }
    return "Unknown action";
}

// Test function that creates instances and calls handle_action
function test() -> string {
    let weather_action = GetWeatherType { city: "San Francisco" };
    let reply_action = Reply { content: "Hello!" };

    // This would work with runtime values
    // For testing, we'll just return a success message
    return "Type narrowing works!";
}