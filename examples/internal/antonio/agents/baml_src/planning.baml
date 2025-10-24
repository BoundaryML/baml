// Activities planning agent

class CityGeoData {
    id string
    name string
    latitude float
    longitude float
    elevation float
    feature_code string
    country_code string
    admin1_id int
    admin2_id int
    admin3_id int
    timezone string
    population int
    postcodes string[]
    country_id int
    country string
    admin1 string
    admin2 string
    admin3 string
}

class GeoApiData {
    results CityGeoData[]
    generationtime_ms float
}

function GetCityGeoData(city: string) -> CityGeoData {
    let url = "https://geocoding-api.open-meteo.com/v1/search?name=" + city + "&count=1";

    let data = baml.fetch_as<GeoApiData>(url);

    data.results[0]
}

class CurrentUnits {
    time string
    interval string
    precipitation string
    weathercode string
}

class HourlyUnits {
    time string
    precipitation_probability string
    temperature_2m string
}

class Current {
    time string
    interval int
    precipitation float
    weathercode int
}

class Hourly {
    time string[]
    precipitation_probability float[]
    temperature_2m float[]
}

class WeatherApiData {
    latitude float
    longitude float
    generationtime_ms float
    utc_offset_seconds int
    timezone string
    timezone_abbreviation string
    elevation float
    current_units CurrentUnits
    current Current
    hourly_units HourlyUnits
    hourly Hourly
}

class Forecast {
    date string
    max_temp float
    min_temp float
    condition string
    location string
    precipitation_chance float
}

function GetWeatherCondition(weathercode: int) -> string {
    if (weathercode == 0) {
        return "Clear";
    } else if (weathercode == 1) {
        return "Mainly clear";
    } else if (weathercode == 2) {
        return "Partly cloudy";
    } else if (weathercode == 3) {
        return "Overcast";
    } else if (weathercode == 45) {
        return "Foggy";
    } else if (weathercode == 48) {
        return "Depositing rime fog";
    } else if (weathercode == 51) {
        return "Light drizzle";
    } else if (weathercode == 53) {
        return "Moderate drizzle";
    } else if (weathercode == 55) {
        return "Dense drizzle";
    } else if (weathercode == 61) {
        return "Slight rain";
    } else if (weathercode == 63) {
        return "Moderate rain";
    } else if (weathercode == 65) {
        return "Heavy rain";
    } else if (weathercode == 71) {
        return "Slight snow fall";
    } else if (weathercode == 73) {
        return "Moderate snow fall";
    } else if (weathercode == 75) {
        return "Heavy snow fall";
    } else if (weathercode == 95) {
        return "Thunderstorm";
    }

    return "Unknown";
}

function GetCityWeatherData(city: string) -> Forecast {
    let geo_data = GetCityGeoData(city);
    let url = "https://api.open-meteo.com/v1/forecast?latitude=" + baml.unstable.string(geo_data.latitude) + "&longitude=" + baml.unstable.string(geo_data.longitude) + "&current=precipitation,weathercode&timezone=auto,&hourly=precipitation_probability,temperature_2m";

    let weather_data = baml.fetch_as<WeatherApiData>(url);

    let max_temp = 0.0;
    for (let t in weather_data.hourly.temperature_2m) {
        if (t > max_temp) {
            max_temp = t;
        }
    }

    let min_temp = 0.0;
    for (let t in weather_data.hourly.temperature_2m) {
        if (t < min_temp) {
            min_temp = t;
        }
    }

    let precipitation_chance = 0.0;
    for (let p in weather_data.hourly.precipitation_probability) {
        if (p > precipitation_chance) {
            precipitation_chance = p;
        }
    }

    let condition = GetWeatherCondition(weather_data.current.weathercode);

    Forecast {
        date: weather_data.current.time,
        max_temp: max_temp,
        min_temp: min_temp,
        condition: condition,
        location: geo_data.name,
        precipitation_chance: precipitation_chance,
    }
}

function PlanningAgent(forecast: Forecast) -> string {
    client CustomGPT5Mini
    prompt #"
        You are a local activities and travel expert who excels at weather-based planning. Analyze the weather data and provide practical activity recommendations.

        📅 [Day, Month Date, Year]
        ═══════════════════════════

        🌡️ WEATHER SUMMARY
        • Conditions: [brief description]
        • Temperature: [X°C/Y°F to A°C/B°F]
        • Precipitation: [X% chance]

        🌅 MORNING ACTIVITIES
        Outdoor:
        • [Activity Name] - [Brief description including specific location/route]
          Best timing: [specific time range]
          Note: [relevant weather consideration]

        🌞 AFTERNOON ACTIVITIES
        Outdoor:
        • [Activity Name] - [Brief description including specific location/route]
          Best timing: [specific time range]
          Note: [relevant weather consideration]

        🏠 INDOOR ALTERNATIVES
        • [Activity Name] - [Brief description including specific venue]
          Ideal for: [weather condition that would trigger this alternative]

        ⚠️ SPECIAL CONSIDERATIONS
        • [Any relevant weather warnings, UV index, wind conditions, etc.]

        Guidelines:
        - Suggest 2-3 time-specific outdoor activities per day
        - Include 1-2 indoor backup options
        - For precipitation >50%, lead with indoor activities
        - All activities must be specific to the location
        - Include specific venues, trails, or locations
        - Consider activity intensity based on temperature
        - Keep descriptions concise but informative

        Maintain this exact formatting for consistency, using the emoji and section headers as shown.

        Here is the weather data:
        {{ forecast }}
    "#
}

function ExtractCityFromUserInput(user_input: string) -> string {
    client CustomGPT5Mini
    prompt #"
        You are a helpful activity planning assistant. Extract the city name
        from the user input so that we can make an API call to get the weather data.
        Answer only with the city name, no other text.

        User input:
        {{ user_input }}
    "#
}

function ActivityPlanningWorkflow(user_input: string) -> string {
    let city = ExtractCityFromUserInput(user_input);
    let forecast = GetCityWeatherData(city);
    let plan = PlanningAgent(forecast);

    return plan;
}