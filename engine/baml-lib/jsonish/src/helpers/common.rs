use baml_types::{LiteralValue, TypeIR};
use internal_baml_jinja::types::{Builder, OutputFormatContent};

pub const CLASS_SCHEMA: &str = r#"
class Book {
    title string
    author string
    year int
    tags string[]
    ratings Rating[]
}

class Rating {
    score int
    reviewer string
    date string
}
"#;

pub const UNION_SCHEMA: &str = r#"
class TextContent {
    text string
}

class ImageContent {
    url string
    width int
    height int
}

class VideoContent {
    url string
    duration int
}

class AudioContent {
    type string
    url string
    duration int
}

type JSONValue = int | float | bool | string | null | JSONValue[] | map<string, JSONValue>

type JSON = int | float | bool | string | JSON[] | map<string, JSON>
class Story2 {
  content map<string, string>
}

class Story1 {
  content string
}

class Story3 {
  content map<string, JSON>
}

class Story4 {
    content JSON
}
"#;

pub const JSON_STRING: &str = r#"
    {
        "number": 1,
        "string": "test",
        "bool": true,
        "list": [1, 2, 3]
    }
"#;

pub const JSON_STRING_MAP: &str = r#"
    {
        "number": 1,
        "string": "test",
        "bool": true,
        "list": [1, 2, 3],
        "object": {
            "number": 1,
            "string": "test",
            "bool": true,
            "list": [1, 2, 3]
        },
        "json": {
            "number": 1,
            "string": "test",
            "bool": true,
            "list": [1, 2, 3],
            "object": {
                "number": 1,
                "string": "test",
                "bool": true,
                "list": [1, 2, 3]
            }
        }
    }
"#;

pub const JSON_STRING_STORY: &str = "{\n  \"content\": {\n    \"title\": \"Whiskers and Scales\",\n    \"setting\": {\n      \"village\": \"Thornfield\",\n      \"forest\": \"Glimmerwood\",\n      \"castle\": \"Skyreach Castle\"\n    },\n    \"characters\": [\n      {\n        \"name\": \"Whiskers\",\n        \"type\": \"cat\",\n        \"traits\": [\"curious\", \"adventurous\", \"brave\"]\n      },\n      {\n        \"name\": \"Aurelius\",\n        \"type\": \"dragon\",\n        \"traits\": [\"wise\", \"noble\", \"gentle\"]\n      }\n    ],\n    \"plot\": [\n      {\n        \"scene\": 1,\n        \"description\": \"In the quaint village of Thornfield, there lived a curious cat named Whiskers. He was known for his adventurous spirit and often explored the nearby Glimmerwood.\"\n      },\n      {\n        \"scene\": 2,\n        \"description\": \"One day, Whiskers heard whispers among the villagers about a dragon residing in Skyreach Castle, which hovered above Thornfield, hidden among the clouds. The villagers feared the dragon, but Whiskers felt an urge to see this majestic creature for himself.\"\n      },\n      {\n        \"scene\": 3,\n        \"description\": \"Taking a deep breath, Whiskers set out for the Glimmerwood, believing the tales that the forest held a hidden path to Skyreach Castle. He navigated through the dense trees, guided by the shimmering glow of Glimmerwood's enchanted flora.\"\n      },\n      {\n        \"scene\": 4,\n        \"description\": \"After a day's journey, Whiskers stumbled upon an ancient, cobblestone pathway. He followed it, and to his astonishment, it led to the base of a spiraling staircase made of clouds.\"\n      },\n      {\n        \"scene\": 5,\n        \"description\": \"The staircase seemed daunting at first, but Whiskers, driven by curiosity and courage, began his ascent. The higher he climbed, the more the village below disappeared under a blanket of mist.\"\n      },\n      {\n        \"scene\": 6,\n        \"description\": \"Upon reaching the top, Whiskers was greeted by the breathtaking sight of Skyreach Castle. Majestic and ancient, the castle glistened in the sunlight.\"\n      },\n      {\n        \"scene\": 7,\n        \"description\": \"Within the castle’s vast courtyard, a shadow moved gracefully. It was Aurelius, an ancient dragon with scales that shimmered like molten gold. Despite his imposing size, there was a gentle, wise aura about him.\"\n      },\n      {\n        \"scene\": 8,\n        \"description\": \"Aurelius noticed Whiskers standing on the edge of the courtyard, curious yet cautious. The dragon lowered his head respectfully, his soft eyes meeting Whiskers'.\"\n      },\n      {\n        \"scene\": 9,\n        \"dialogue\": {\n          \"Aurelius\": \"Greetings, little one. What brings you to this lofty realm?\",\n          \"Whiskers\": \"I wished to see the truth behind the tales, noble dragon. I seek knowledge and a friend, perhaps.\"\n        }\n      },\n      {\n        \"scene\": 10,\n        \"description\": \"Aurelius chuckled, a deep, rumbling sound like distant thunder. He admired the cat's bravery. 'You have found both,' he replied, offering his friendship.\"\n      },\n      {\n        \"scene\": 11,\n        \"description\": \"Their conversations meandered through stories of old, of dragon lore, and the secrets of the skies. Whiskers learned much from Aurelius, eagerly lapping up the dragon's wisdom.\"\n      },\n      {\n        \"scene\": 12,\n        \"description\": \"Days passed into weeks, as Whiskers and Aurelius formed a bond stronger than any Whiskers ever knew. Under the dragon’s tutelage, Whiskers discovered his own hidden potentials; the courage and heart of a true adventurer.\"\n      },\n      {\n        \"scene\": 13,\n        \"description\": \"Realizing he must return and share his newfound knowledge, Whiskers bid Aurelius farewell, promising to visit often.\"\n      },\n      {\n        \"scene\": 14,\n        \"description\": \"Returning to Thornfield, Whiskers shared stories of his friendship with Aurelius. His tales transformed the villagers' fear into awe and respect for the dragon of Skyreach Castle.\"\n      },\n      {\n        \"scene\": 15,\n        \"conclusion\": \"Thus, Whiskers and Aurelius cemented an alliance between earth and sky, reminding all that true friendships can bridge even the gap between humble cats and mighty dragons.\"\n      }\n    ]\n  }\n}";
