
class Weather {
  city string
  temperature int
}

fn GetWheatherValue(city: string) -> Weather {
  std::fetch_value<Weather>(1)  
}