enum RenderTestEnum {
  BIKE @alias("two-wheeled bike")
  SCOOTER @alias("kick scooter")
  @@dynamic
}

class RenderEnumInput {
  testKey string
  @@dynamic
}

function RenderDynamicEnum(bike: RenderTestEnum, other: RenderTestEnum) -> string {
  client GPT35
  prompt #"
    "RenderTestEnum.BIKE" renders as: {{ bike }}
    "other" renders as: {{ other }}
    
    Available dynamic enum values:
      - BIKE: {{ RenderTestEnum.BIKE }}
      - SCOOTER: {{ RenderTestEnum.SCOOTER }}

    Enum comparison tests:

    {% if bike == RenderTestEnum.BIKE %}
    'bike' is equal to RenderTestEnum.BIKE, as expected
    {% else %}
    'bike' should be equal to RenderTestEnum.BIKE, but is not
    {% endif %}

    {% if bike != RenderTestEnum.SCOOTER %}
    'bike' is not equal to RenderTestEnum.SCOOTER, as expected
    {% else %}
    'bike' should not be equal to RenderTestEnum.SCOOTER, but it is
    {% endif %}

    {% if bike == "BIKE" %}
    'bike' equals "BIKE", as expected
    {% else %}
    'bike' should equal "BIKE", but does not
    {% endif %}

    {% if bike != "SCOOTER" %}
    'bike' is not equal to "SCOOTER", as expected
    {% else %}
    'bike' should be equal to "SCOOTER", but is not
    {% endif %}

    Multiple value tests:

    {% if bike == RenderTestEnum.BIKE or bike == RenderTestEnum.SCOOTER %}
    'bike' is equal to RenderTestEnum.BIKE or RenderTestEnum.SCOOTER, as expected
    {% else %}
    'bike' should be equal to RenderTestEnum.BIKE or RenderTestEnum.SCOOTER, but it is not
    {% endif %}

    {% if other != RenderTestEnum.BIKE %}
    'other' is not equal to RenderTestEnum.BIKE, as expected
    {% else %}
    'other' should not be equal to RenderTestEnum.BIKE, but it is
    {% endif %}

    {% if other == "MOTORCYCLE" %}
    'other' is MOTORCYCLE, as expected
    {% else %}
    'other' should be MOTORCYCLE, but it is not
    {% endif %}

    {% if other.value == "MOTORCYCLE" %}
    'other' is MOTORCYCLE, as expected
    {% else %}
    'other' should be MOTORCYCLE, but it is not
    {% endif %}
  "#
}