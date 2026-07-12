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
    <city>London</city>
    <zip>12345</zip>
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
    r#"<items><item>one</item><item>two</item></items>"#,
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
  <address><city>New York</city><zip>10001</zip></address>
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
    <value>2</value>
    <next>null</next>"#,
    TypeIR::class("Node"),
    {"value": 1, "next": {"value": 2, "next": null}}
);

test_deserializer!(
    xml_unicode_field_alias,
    XML_TYPES,
    r#"<International><prénom>Ada</prénom></International>"#,
    TypeIR::class("International"),
    {"name": "Ada"}
);
