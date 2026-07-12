use baml_types::ir_type::UnionConstructor;

use super::*;

const XML_TYPES: &str = r#"
enum Status {
  ACTIVE
  INACTIVE
}

class Address {
  city string
  zip int
}

class Person {
  name string
  status Status
  tags string[]
  address Address
  nickname string?
  metadata map<string, int>
  verified int | bool
}

class Node {
  value int
  next Node?
}

class International {
  name string @alias("prénom")
}

class Cat {
  name string
  lives int
}

class Dog {
  name string
  trained bool
}

class Bird {
  species string
  canFly bool
}

class Habitat {
  residents (Cat | Dog | Bird)[]
  featured Cat | Dog | Bird
}

class Directory {
  addresses Address[]
}

class NullableRecord {
  required string
  optionalText string?
  optionalAddress Address?
  nullablePet Cat | Dog | null
  omittedPet Cat | Dog | null
}
"#;

test_deserializer!(
    xml_class_enum_list_map_optional_and_union,
    XML_TYPES,
    r#"The extracted value is:
<Person>
  <name>Ada Lovelace</name>
  <status>ACTIVE</status>
  <tags>
    <item>mathematics</item>
    <item>programming</item>
  </tags>
  <address>
    <Address>
      <city>London</city>
      <zip>12345</zip>
    </Address>
  </address>
  <metadata>
    <entry><key>score</key><value>10</value></entry>
    <entry><key>rank</key><value>1</value></entry>
  </metadata>
  <verified>true</verified>
</Person>"#,
    TypeIR::class("Person"),
    {
        "name": "Ada Lovelace",
        "status": "ACTIVE",
        "tags": ["mathematics", "programming"],
        "address": {"city": "London", "zip": 12345},
        "nickname": null,
        "metadata": {"score": 10, "rank": 1},
        "verified": true
    }
);

test_deserializer!(
    xml_top_level_list,
    XML_TYPES,
    r#"<list><item>one</item><item>two</item></list>"#,
    TypeIR::list(TypeIR::string()),
    ["one", "two"]
);

test_deserializer!(
    xml_top_level_enum,
    XML_TYPES,
    r#"<?xml version="1.0"?><Status>INACTIVE</Status>"#,
    TypeIR::r#enum("Status"),
    "INACTIVE"
);

test_deserializer!(
    xml_optional_explicit_null,
    XML_TYPES,
    r#"<Person>
  <name>Grace Hopper</name>
  <status>ACTIVE</status>
  <tags></tags>
  <address><Address><city>New York</city><zip>10001</zip></Address></address>
  <nickname>null</nickname>
  <metadata></metadata>
  <verified>1</verified>
</Person>"#,
    TypeIR::class("Person"),
    {
        "name": "Grace Hopper",
        "status": "ACTIVE",
        "tags": [],
        "address": {"city": "New York", "zip": 10001},
        "nickname": null,
        "metadata": {},
        "verified": 1
    }
);

test_deserializer!(
    xml_recursive_and_recoverable_missing_close_tags,
    XML_TYPES,
    r#"Here is the result:
<Node>
  <value>1</value>
  <next>
    <Node>
      <value>2</value>
      <next>null</next>
    </Node>"#,
    TypeIR::class("Node"),
    {"value": 1, "next": {"value": 2, "next": null}}
);

test_deserializer!(
    xml_deeply_recursive_class,
    XML_TYPES,
    r#"<Node>
  <value>1</value>
  <next>
    <Node>
      <value>2</value>
      <next>
        <Node>
          <value>3</value>
          <next>
            <Node>
              <value>4</value>
              <next>
                <Node>
                  <value>5</value>
                  <next>null</next>
                </Node>
              </next>
            </Node>
          </next>
        </Node>
      </next>
    </Node>
  </next>
</Node>"#,
    TypeIR::class("Node"),
    {
        "value": 1,
        "next": {
            "value": 2,
            "next": {
                "value": 3,
                "next": {
                    "value": 4,
                    "next": {"value": 5, "next": null}
                }
            }
        }
    }
);

test_deserializer!(
    xml_array_of_classes,
    XML_TYPES,
    r#"<AddressList>
  <Address><city>London</city><zip>12345</zip></Address>
  <Address><city>Paris</city><zip>75001</zip></Address>
  <Address><city>Tokyo</city><zip>100</zip></Address>
</AddressList>"#,
    TypeIR::list(TypeIR::class("Address")),
    [
        {"city": "London", "zip": 12345},
        {"city": "Paris", "zip": 75001},
        {"city": "Tokyo", "zip": 100}
    ]
);

test_deserializer!(
    xml_array_of_class_unions,
    XML_TYPES,
    r#"<list>
  <Cat><name>Milo</name><lives>9</lives></Cat>
  <Dog><name>Rex</name><trained>true</trained></Dog>
  <Bird><species>macaw</species><canFly>true</canFly></Bird>
</list>"#,
    TypeIR::list(TypeIR::union(vec![
        TypeIR::class("Cat"),
        TypeIR::class("Dog"),
        TypeIR::class("Bird"),
    ])),
    [
        {"name": "Milo", "lives": 9},
        {"name": "Rex", "trained": true},
        {"species": "macaw", "canFly": true}
    ]
);

test_deserializer!(
    xml_nested_arrays,
    XML_TYPES,
    r#"<list>
  <item><item>1</item><item>2</item><item>3</item></item>
  <item><item>4</item><item>5</item></item>
  <item><item>6</item></item>
  <item></item>
</list>"#,
    TypeIR::list(TypeIR::list(TypeIR::int())),
    [[1, 2, 3], [4, 5], [6], []]
);

test_deserializer!(
    xml_union_with_multiple_class_variants,
    XML_TYPES,
    r#"<Dog><name>Rex</name><trained>true</trained></Dog>"#,
    TypeIR::union(vec![
        TypeIR::class("Cat"),
        TypeIR::class("Dog"),
        TypeIR::class("Bird"),
    ]),
    {"name": "Rex", "trained": true}
);

test_deserializer!(
    xml_class_with_array_and_class_union,
    XML_TYPES,
    r#"<Habitat>
  <residents>
    <Cat><name>Milo</name><lives>9</lives></Cat>
    <Dog><name>Rex</name><trained>false</trained></Dog>
    <Bird><species>penguin</species><canFly>false</canFly></Bird>
  </residents>
  <featured><Bird><species>owl</species><canFly>true</canFly></Bird></featured>
</Habitat>"#,
    TypeIR::class("Habitat"),
    {
        "residents": [
            {"name": "Milo", "lives": 9},
            {"name": "Rex", "trained": false},
            {"species": "penguin", "canFly": false}
        ],
        "featured": {"species": "owl", "canFly": true}
    }
);

test_deserializer!(
    xml_class_with_class_list_field,
    XML_TYPES,
    r#"<Directory>
  <addresses>
    <AddressList>
      <Address><city>London</city><zip>12345</zip></Address>
      <Address><city>Paris</city><zip>75001</zip></Address>
    </AddressList>
  </addresses>
</Directory>"#,
    TypeIR::class("Directory"),
    {
        "addresses": [
            {"city": "London", "zip": 12345},
            {"city": "Paris", "zip": 75001}
        ]
    }
);

test_deserializer!(
    xml_optional_and_nullable_class_fields,
    XML_TYPES,
    r#"<NullableRecord>
  <required>present</required>
  <optionalAddress>null</optionalAddress>
  <nullablePet>null</nullablePet>
</NullableRecord>"#,
    TypeIR::class("NullableRecord"),
    {
        "required": "present",
        "optionalText": null,
        "optionalAddress": null,
        "nullablePet": null,
        "omittedPet": null
    }
);

test_deserializer!(
    xml_optional_class_field_with_class_wrapper,
    XML_TYPES,
    r#"<NullableRecord>
  <required>present</required>
  <optionalAddress>
    <Address><city>London</city><zip>12345</zip></Address>
  </optionalAddress>
</NullableRecord>"#,
    TypeIR::class("NullableRecord"),
    {
        "required": "present",
        "optionalText": null,
        "optionalAddress": {"city": "London", "zip": 12345},
        "nullablePet": null,
        "omittedPet": null
    }
);

test_deserializer!(
    xml_map_with_class_values,
    XML_TYPES,
    r#"<map>
  <entry>
    <key>home</key>
    <value><Address><city>London</city><zip>12345</zip></Address></value>
  </entry>
  <entry>
    <key>office</key>
    <value><Address><city>Paris</city><zip>75001</zip></Address></value>
  </entry>
</map>"#,
    TypeIR::map(TypeIR::string(), TypeIR::class("Address")),
    {
        "home": {"city": "London", "zip": 12345},
        "office": {"city": "Paris", "zip": 75001}
    }
);

test_deserializer!(
    xml_recoverable_deep_missing_close_tags_with_preamble,
    XML_TYPES,
    r#"I found the requested recursive structure.

<Node>
  <value>10</value>
  <next>
    <Node>
      <value>20</value>
      <next>
        <Node>
          <value>30</value>
          <next>null</next>"#,
    TypeIR::class("Node"),
    {"value": 10, "next": {"value": 20, "next": {"value": 30, "next": null}}}
);

test_deserializer!(
    xml_recoverable_extra_closing_tags,
    XML_TYPES,
    r#"The requested address is below:
<Address><city>Lisbon</city><zip>1100</zip></Address></Address></result>"#,
    TypeIR::class("Address"),
    {"city": "Lisbon", "zip": 1100}
);

test_deserializer!(
    xml_unicode_field_alias,
    XML_TYPES,
    r#"<International><prénom>Ada</prénom></International>"#,
    TypeIR::class("International"),
    {"name": "Ada"}
);
