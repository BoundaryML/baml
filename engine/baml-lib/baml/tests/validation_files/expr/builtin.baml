class Weather {
  city string
  temperature int
}

fn GetWheatherValue(city: string) -> Weather {
  std::fetch_value<Weather>(std::request {
    base_url: "api.weather.com",
    query_params: null,
    body: { "city": city },
  })
}
