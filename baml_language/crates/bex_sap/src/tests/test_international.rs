use crate::{baml_db, baml_tyannotated};

// =============================================================================
// ENUM TESTS WITH ACCENTED ALIASES
// =============================================================================

test_deserializer!(
    test_accented_alias_french,
    r#"française"#,
    baml_tyannotated!(CuisineType),
    baml_db! {
        enum CuisineType {
            FRENCH @alias("française"),
            SPANISH @alias("española"),
            PORTUGUESE @alias("portuguesa"),
            ITALIAN @alias("italiana"),
            CHINESE @alias("中式"),
            ARABIC @alias("العربية"),
            RUSSIAN @alias("русская"),
            JAPANESE @alias("日本料理")
        }
    },
    "FRENCH"
);

test_deserializer!(
    test_accented_alias_spanish,
    r#"española"#,
    baml_tyannotated!(CuisineType),
    baml_db! {
        enum CuisineType {
            FRENCH @alias("française"),
            SPANISH @alias("española"),
            PORTUGUESE @alias("portuguesa"),
            ITALIAN @alias("italiana"),
            CHINESE @alias("中式"),
            ARABIC @alias("العربية"),
            RUSSIAN @alias("русская"),
            JAPANESE @alias("日本料理")
        }
    },
    "SPANISH"
);

test_deserializer!(
    test_accented_alias_chinese,
    r#"中式"#,
    baml_tyannotated!(CuisineType),
    baml_db! {
        enum CuisineType {
            FRENCH @alias("française"),
            SPANISH @alias("española"),
            PORTUGUESE @alias("portuguesa"),
            ITALIAN @alias("italiana"),
            CHINESE @alias("中式"),
            ARABIC @alias("العربية"),
            RUSSIAN @alias("русская"),
            JAPANESE @alias("日本料理")
        }
    },
    "CHINESE"
);

test_deserializer!(
    test_accented_alias_arabic,
    r#"العربية"#,
    baml_tyannotated!(CuisineType),
    baml_db! {
        enum CuisineType {
            FRENCH @alias("française"),
            SPANISH @alias("española"),
            PORTUGUESE @alias("portuguesa"),
            ITALIAN @alias("italiana"),
            CHINESE @alias("中式"),
            ARABIC @alias("العربية"),
            RUSSIAN @alias("русская"),
            JAPANESE @alias("日本料理")
        }
    },
    "ARABIC"
);

test_deserializer!(
    test_accented_alias_russian,
    r#"русская"#,
    baml_tyannotated!(CuisineType),
    baml_db! {
        enum CuisineType {
            FRENCH @alias("française"),
            SPANISH @alias("española"),
            PORTUGUESE @alias("portuguesa"),
            ITALIAN @alias("italiana"),
            CHINESE @alias("中式"),
            ARABIC @alias("العربية"),
            RUSSIAN @alias("русская"),
            JAPANESE @alias("日本料理")
        }
    },
    "RUSSIAN"
);

test_deserializer!(
    test_accented_alias_japanese,
    r#"日本料理"#,
    baml_tyannotated!(CuisineType),
    baml_db! {
        enum CuisineType {
            FRENCH @alias("française"),
            SPANISH @alias("española"),
            PORTUGUESE @alias("portuguesa"),
            ITALIAN @alias("italiana"),
            CHINESE @alias("中式"),
            ARABIC @alias("العربية"),
            RUSSIAN @alias("русская"),
            JAPANESE @alias("日本料理")
        }
    },
    "JAPANESE"
);

test_failing_deserializer!(
    test_original_enum_values_fail_when_aliases_exist,
    r#"FRENCH"#,
    baml_tyannotated!(CuisineType),
    baml_db! {
        enum CuisineType {
            FRENCH @alias("française"),
            SPANISH @alias("española"),
            PORTUGUESE @alias("portuguesa"),
            ITALIAN @alias("italiana"),
            CHINESE @alias("中式"),
            ARABIC @alias("العربية"),
            RUSSIAN @alias("русская"),
            JAPANESE @alias("日本料理")
        }
    }
);

test_deserializer!(
    test_accented_alias_in_sentence,
    r#"The restaurant serves española cuisine with authentic flavors"#,
    baml_tyannotated!(CuisineType),
    baml_db! {
        enum CuisineType {
            FRENCH @alias("française"),
            SPANISH @alias("española"),
            PORTUGUESE @alias("portuguesa"),
            ITALIAN @alias("italiana"),
            CHINESE @alias("中式"),
            ARABIC @alias("العربية"),
            RUSSIAN @alias("русская"),
            JAPANESE @alias("日本料理")
        }
    },
    "SPANISH"
);

test_deserializer!(
    test_accented_alias_case_variations,
    r#"Française"#,
    baml_tyannotated!(CuisineType),
    baml_db! {
        enum CuisineType {
            FRENCH @alias("française"),
            SPANISH @alias("española"),
            PORTUGUESE @alias("portuguesa"),
            ITALIAN @alias("italiana"),
            CHINESE @alias("中式"),
            ARABIC @alias("العربية"),
            RUSSIAN @alias("русская"),
            JAPANESE @alias("日本料理")
        }
    },
    "FRENCH"
);

test_deserializer!(
    test_accented_alias_list_mixed_scripts,
    r#"["française", "中式", "العربية"]"#,
    baml_tyannotated!([CuisineType]),
    baml_db! {
        enum CuisineType {
            FRENCH @alias("française"),
            SPANISH @alias("española"),
            PORTUGUESE @alias("portuguesa"),
            ITALIAN @alias("italiana"),
            CHINESE @alias("中式"),
            ARABIC @alias("العربية"),
            RUSSIAN @alias("русская"),
            JAPANESE @alias("日本料理")
        }
    },
    ["FRENCH", "CHINESE", "ARABIC"]
);

test_deserializer!(
    test_french_alias_invoice,
    r#"facture"#,
    baml_tyannotated!(DocumentType),
    baml_db! {
        enum DocumentType {
            INVOICE @alias("facture"),
            RECEIPT @alias("reçu"),
            CONTRACT @alias("contrat"),
            REPORT @alias("rapport"),
            LETTER @alias("lettre")
        }
    },
    "INVOICE"
);

test_deserializer!(
    test_french_alias_receipt_with_accent,
    r#"reçu"#,
    baml_tyannotated!(DocumentType),
    baml_db! {
        enum DocumentType {
            INVOICE @alias("facture"),
            RECEIPT @alias("reçu"),
            CONTRACT @alias("contrat"),
            REPORT @alias("rapport"),
            LETTER @alias("lettre")
        }
    },
    "RECEIPT"
);

test_failing_deserializer!(
    test_original_enum_values_fail_with_french_aliases,
    r#"INVOICE"#,
    baml_tyannotated!(DocumentType),
    baml_db! {
        enum DocumentType {
            INVOICE @alias("facture"),
            RECEIPT @alias("reçu"),
            CONTRACT @alias("contrat"),
            REPORT @alias("rapport"),
            LETTER @alias("lettre")
        }
    }
);

test_deserializer!(
    test_french_alias_in_context,
    r#"Please process this facture document"#,
    baml_tyannotated!(DocumentType),
    baml_db! {
        enum DocumentType {
            INVOICE @alias("facture"),
            RECEIPT @alias("reçu"),
            CONTRACT @alias("contrat"),
            REPORT @alias("rapport"),
            LETTER @alias("lettre")
        }
    },
    "INVOICE"
);

test_deserializer!(
    test_status_french_alias_active,
    r#"actif"#,
    baml_tyannotated!(Status),
    baml_db! {
        enum Status {
            ACTIVE @alias("actif"),
            INACTIVE @alias("inactif"),
            PENDING @alias("en_attente"),
            COMPLETED @alias("terminé"),
            CANCELLED @alias("annulé")
        }
    },
    "ACTIVE"
);

test_deserializer!(
    test_status_french_alias_completed,
    r#"terminé"#,
    baml_tyannotated!(Status),
    baml_db! {
        enum Status {
            ACTIVE @alias("actif"),
            INACTIVE @alias("inactif"),
            PENDING @alias("en_attente"),
            COMPLETED @alias("terminé"),
            CANCELLED @alias("annulé")
        }
    },
    "COMPLETED"
);

test_deserializer!(
    test_status_french_alias_cancelled,
    r#"annulé"#,
    baml_tyannotated!(Status),
    baml_db! {
        enum Status {
            ACTIVE @alias("actif"),
            INACTIVE @alias("inactif"),
            PENDING @alias("en_attente"),
            COMPLETED @alias("terminé"),
            CANCELLED @alias("annulé")
        }
    },
    "CANCELLED"
);

test_deserializer!(
    test_priority_french_high,
    r#"élevé"#,
    baml_tyannotated!(Priority),
    baml_db! {
        enum Priority {
            HIGH @alias("élevé"),
            MEDIUM @alias("médium"),
            LOW @alias("baixo"),
            URGENT @alias("紧急"),
            NORMAL @alias("عادي")
        }
    },
    "HIGH"
);

test_deserializer!(
    test_priority_french_medium,
    r#"médium"#,
    baml_tyannotated!(Priority),
    baml_db! {
        enum Priority {
            HIGH @alias("élevé"),
            MEDIUM @alias("médium"),
            LOW @alias("baixo"),
            URGENT @alias("紧急"),
            NORMAL @alias("عادي")
        }
    },
    "MEDIUM"
);

test_deserializer!(
    test_priority_portuguese_low,
    r#"baixo"#,
    baml_tyannotated!(Priority),
    baml_db! {
        enum Priority {
            HIGH @alias("élevé"),
            MEDIUM @alias("médium"),
            LOW @alias("baixo"),
            URGENT @alias("紧急"),
            NORMAL @alias("عادي")
        }
    },
    "LOW"
);

test_deserializer!(
    test_priority_chinese_urgent,
    r#"紧急"#,
    baml_tyannotated!(Priority),
    baml_db! {
        enum Priority {
            HIGH @alias("élevé"),
            MEDIUM @alias("médium"),
            LOW @alias("baixo"),
            URGENT @alias("紧急"),
            NORMAL @alias("عادي")
        }
    },
    "URGENT"
);

test_deserializer!(
    test_priority_arabic_normal,
    r#"عادي"#,
    baml_tyannotated!(Priority),
    baml_db! {
        enum Priority {
            HIGH @alias("élevé"),
            MEDIUM @alias("médium"),
            LOW @alias("baixo"),
            URGENT @alias("紧急"),
            NORMAL @alias("عادي")
        }
    },
    "NORMAL"
);

test_failing_deserializer!(
    test_original_priority_values_fail_with_aliases,
    r#"HIGH"#,
    baml_tyannotated!(Priority),
    baml_db! {
        enum Priority {
            HIGH @alias("élevé"),
            MEDIUM @alias("médium"),
            LOW @alias("baixo"),
            URGENT @alias("紧急"),
            NORMAL @alias("عادي")
        }
    }
);

// original values are not allowed in lists
// so we should return an empty list
test_deserializer!(
    test_multiple_original_enum_values_fail,
    r#"["HIGH", "MEDIUM", "LOW"]"#,
    baml_tyannotated!([Priority]),
    baml_db! {
        enum Priority {
            HIGH @alias("élevé"),
            MEDIUM @alias("médium"),
            LOW @alias("baixo"),
            URGENT @alias("紧急"),
            NORMAL @alias("عادي")
        }
    },
    // since "médium" -> "MEDIUM"
    ["MEDIUM"]
);

// =============================================================================
// LITERAL TESTS WITH ACCENTED VALUES
// =============================================================================

test_deserializer!(
    test_literal_string_french_name,
    r#"François"#,
    baml_tyannotated!("François"),
    baml_db! {},
    "François"
);

test_deserializer!(
    test_literal_string_spanish_greeting,
    r#"¡Hola!"#,
    baml_tyannotated!("¡Hola!"),
    baml_db! {},
    "¡Hola!"
);

test_deserializer!(
    test_literal_string_portuguese_word,
    r#"São Paulo"#,
    baml_tyannotated!("São Paulo"),
    baml_db! {},
    "São Paulo"
);

test_deserializer!(
    test_literal_string_german_umlaut,
    r#"Müller"#,
    baml_tyannotated!("Müller"),
    baml_db! {},
    "Müller"
);

test_deserializer!(
    test_literal_string_chinese_characters,
    r#"北京"#,
    baml_tyannotated!("北京"),
    baml_db! {},
    "北京"
);

test_deserializer!(
    test_literal_string_arabic_text,
    r#"السلام عليكم"#,
    baml_tyannotated!("السلام عليكم"),
    baml_db! {},
    "السلام عليكم"
);

test_deserializer!(
    test_literal_string_russian_cyrillic,
    r#"Москва"#,
    baml_tyannotated!("Москва"),
    baml_db! {},
    "Москва"
);

test_deserializer!(
    test_literal_string_japanese_hiragana,
    r#"こんにちは"#,
    baml_tyannotated!("こんにちは"),
    baml_db! {},
    "こんにちは"
);

test_deserializer!(
    test_literal_string_accented_with_quotes,
    r#""François""#,
    baml_tyannotated!("François"),
    baml_db! {},
    "François"
);

test_deserializer!(
    test_literal_string_accented_case_insensitive,
    r#"françois"#,
    baml_tyannotated!("François"),
    baml_db! {},
    "François"
);

test_deserializer!(
    test_literal_string_accented_in_sentence,
    r#"The name is François for this person"#,
    baml_tyannotated!("François"),
    baml_db! {},
    "François"
);

test_deserializer!(
    test_literal_string_cafe_with_emoji,
    r#"Café ☕"#,
    baml_tyannotated!("Café ☕"),
    baml_db! {},
    "Café ☕"
);

test_deserializer!(
    test_union_literal_city_names,
    r#"São Paulo"#,
    baml_tyannotated!(("Paris" | "São Paulo" | "Zürich")),
    baml_db! {},
    "São Paulo"
);

test_deserializer!(
    test_union_literal_mixed_languages,
    r#"北京"#,
    baml_tyannotated!(("Paris" | "北京" | "القاهرة")),
    baml_db! {},
    "北京"
);

test_deserializer!(
    test_literal_string_diacritics_combination,
    r#"naïve résumé"#,
    baml_tyannotated!("naïve résumé"),
    baml_db! {},
    "naïve résumé"
);

// =============================================================================
// CLASS TESTS WITH ACCENTED ALIASES
// =============================================================================

test_deserializer!(
  test_french_field_aliases,
  r#"{"nom": "Le Petit Café", "adresse": "Champs-Élysées", "spécialité": "crêpes bretonnes", "étoiles": 4}"#,
  baml_tyannotated!(Restaurant),
  baml_db!{
      class Restaurant {
          name: string @alias("nom"),
          address: string @alias("adresse"),
          specialty: string @alias("spécialité"),
          stars: int @alias("étoiles"),
      }
  },
  {
    "name": "Le Petit Café",
    "address": "Champs-Élysées",
    "specialty": "crêpes bretonnes",
    "stars": 4
  }
);

test_deserializer!(
  test_french_field_aliases_without_quotes,
  r#"{nom: "Le Petit Café", adresse: Champs-Élysées, spécialité: "crêpes bretonnes", étoiles: 4}"#,
  baml_tyannotated!(Restaurant),
  baml_db!{
      class Restaurant {
          name: string @alias("nom"),
          address: string @alias("adresse"),
          specialty: string @alias("spécialité"),
          stars: int @alias("étoiles"),
      }
  },
  {
    "name": "Le Petit Café",
    "address": "Champs-Élysées",
    "specialty": "crêpes bretonnes",
    "stars": 4
  }
);

test_failing_deserializer!(
    test_original_field_names_fail_when_aliases_exist,
    r#"{"name": "Le Petit Café", "address": "Champs-Élysées", "specialty": "crêpes bretonnes", "stars": 4}"#,
    baml_tyannotated!(Restaurant),
    baml_db! {
        class Restaurant {
            name: string @alias("nom"),
            address: string @alias("adresse"),
            specialty: string @alias("spécialité"),
            stars: int @alias("étoiles"),
        }
    }
);

test_deserializer!(
  test_international_field_aliases,
  r#"{"prénom": "François", "família": "Silva", "città": "Milano", "straße": "Hauptstraße", "поле": "значение", "フィールド": "値"}"#,
  baml_tyannotated!(InternationalContact),
  baml_db!{
      class InternationalContact {
          first_name: string @alias("prénom"),
          family_name: string @alias("família"),
          city: string @alias("città"),
          street: string @alias("straße"),
          field: string @alias("поле"),
          data_field: string @alias("フィールド"),
      }
  },
  {
    "first_name": "François",
    "family_name": "Silva",
    "city": "Milano",
    "street": "Hauptstraße",
    "field": "значение",
    "data_field": "値"
  }
);

test_deserializer!(
  test_international_aliases_with_context,
  r#"Here is the contact information:
  {
    "prénom": "José",
    "família": "González",
    "città": "Barcelona",
    "straße": "Königstraße",
    "поле": "текст",
    "フィールド": "データ"
  }"#,
  baml_tyannotated!(InternationalContact),
  baml_db!{
      class InternationalContact {
          first_name: string @alias("prénom"),
          family_name: string @alias("família"),
          city: string @alias("città"),
          street: string @alias("straße"),
          field: string @alias("поле"),
          data_field: string @alias("フィールド"),
      }
  },
  {
    "first_name": "José",
    "family_name": "González",
    "city": "Barcelona",
    "street": "Königstraße",
    "field": "текст",
    "data_field": "データ"
  }
);

test_deserializer!(
    test_french_nested_class_aliases,
    r#"{"prénom": "François", "nom": "Müller", "âge": 35, "adresse": {"numéro": 42, "rue": "Champs-Élysées", "ville": "Paris", "région": "Île-de-France"}}"#,
    baml_tyannotated!(Person),
    baml_db!{
        class Address {
            number: int @alias("numéro"),
            street: string @alias("rue"),
            city: string @alias("ville"),
            region: string @alias("région"),
        }
        class Person {
            first_name: string @alias("prénom"),
            last_name: string @alias("nom"),
            age: int @alias("âge"),
            address: Address @alias("adresse"),
        }
    },
    {
        "first_name": "François",
        "last_name": "Müller",
        "age": 35,
        "address": {
            "number": 42,
            "street": "Champs-Élysées",
            "city": "Paris",
            "region": "Île-de-France"
        }
    }
);

test_deserializer!(
  test_class_with_accented_aliases,
  r#"{"nom": "Café Latte", "prix": 4.50, "catégorie": "Boissons", "description": "Délicieux café"}"#,
  baml_tyannotated!(ProductInfo),
  baml_db!{
      class ProductInfo {
          name: string @alias("nom"),
          price: float @alias("prix"),
          category: string @alias("catégorie"),
          description: string @alias("description"),
      }
  },
  {
    "name": "Café Latte",
    "price": 4.50,
    "category": "Boissons",
    "description": "Délicieux café"
  }
);

test_failing_deserializer!(
    test_original_field_names_fail_with_aliases,
    r#"{"name": "Café Latte", "price": 4.50, "category": "Boissons", "description": "Délicieux café"}"#,
    baml_tyannotated!(ProductInfo),
    baml_db! {
        class ProductInfo {
            name: string @alias("nom"),
            price: float @alias("prix"),
            category: string @alias("catégorie"),
            description: string @alias("description"),
        }
    }
);

test_deserializer!(
  test_library_with_french_aliases,
  r#"{"livres": ["L'Étranger", "Amélie Poulain", "Naïveté"], "auteurs": ["Camus", "Jeunet", "Müller"], "années": [1942, 2001, 2020]}"#,
  baml_tyannotated!(Library),
  baml_db!{
      class Library {
          books: [string] @alias("livres"),
          authors: [string] @alias("auteurs"),
          years: [int] @alias("années"),
      }
  },
  {
    "books": ["L'Étranger", "Amélie Poulain", "Naïveté"],
    "authors": ["Camus", "Jeunet", "Müller"],
    "years": [1942, 2001, 2020]
  }
);

// =============================================================================
// UNACCENTED INPUT MATCHING ACCENTED ALIASES
// =============================================================================

test_deserializer!(
    test_unaccented_senor_matches_accented_alias,
    r#"senor"#,
    baml_tyannotated!(SpanishTitle),
    baml_db! {
        enum SpanishTitle {
            MISTER @alias("señor"),
            MISS @alias("señorita"),
            DOCTOR @alias("doctor"),
            PROFESSOR @alias("profesor")
        }
    },
    "MISTER"
);

test_deserializer!(
    test_unaccented_senorita_matches_accented_alias,
    r#"senorita"#,
    baml_tyannotated!(SpanishTitle),
    baml_db! {
        enum SpanishTitle {
            MISTER @alias("señor"),
            MISS @alias("señorita"),
            DOCTOR @alias("doctor"),
            PROFESSOR @alias("profesor")
        }
    },
    "MISS"
);

test_deserializer!(
    test_unaccented_profesor_matches_accented_alias,
    r#"profesor"#,
    baml_tyannotated!(SpanishTitle),
    baml_db! {
        enum SpanishTitle {
            MISTER @alias("señor"),
            MISS @alias("señorita"),
            DOCTOR @alias("doctor"),
            PROFESSOR @alias("profesor")
        }
    },
    "PROFESSOR"
);

test_deserializer!(
    test_unaccented_in_sentence,
    r#"The title is senor for this person"#,
    baml_tyannotated!(SpanishTitle),
    baml_db! {
        enum SpanishTitle {
            MISTER @alias("señor"),
            MISS @alias("señorita"),
            DOCTOR @alias("doctor"),
            PROFESSOR @alias("profesor")
        }
    },
    "MISTER"
);

test_deserializer!(
    test_unaccented_cafe_matches_accented_alias,
    r#"cafe"#,
    baml_tyannotated!(FrenchWords),
    baml_db! {
        enum FrenchWords {
            COFFEE @alias("café"),
            NAIVE @alias("naïve"),
            RESUME @alias("résumé"),
            ELITE @alias("élite"),
            FACADE @alias("façade")
        }
    },
    "COFFEE"
);

test_deserializer!(
    test_unaccented_naive_matches_accented_alias,
    r#"naive"#,
    baml_tyannotated!(FrenchWords),
    baml_db! {
        enum FrenchWords {
            COFFEE @alias("café"),
            NAIVE @alias("naïve"),
            RESUME @alias("résumé"),
            ELITE @alias("élite"),
            FACADE @alias("façade")
        }
    },
    "NAIVE"
);

test_deserializer!(
    test_unaccented_resume_matches_accented_alias,
    r#"resume"#,
    baml_tyannotated!(FrenchWords),
    baml_db! {
        enum FrenchWords {
            COFFEE @alias("café"),
            NAIVE @alias("naïve"),
            RESUME @alias("résumé"),
            ELITE @alias("élite"),
            FACADE @alias("façade")
        }
    },
    "RESUME"
);

test_deserializer!(
    test_unaccented_elite_matches_accented_alias,
    r#"elite"#,
    baml_tyannotated!(FrenchWords),
    baml_db! {
        enum FrenchWords {
            COFFEE @alias("café"),
            NAIVE @alias("naïve"),
            RESUME @alias("résumé"),
            ELITE @alias("élite"),
            FACADE @alias("façade")
        }
    },
    "ELITE"
);

test_deserializer!(
    test_unaccented_facade_matches_accented_alias,
    r#"facade"#,
    baml_tyannotated!(FrenchWords),
    baml_db! {
        enum FrenchWords {
            COFFEE @alias("café"),
            NAIVE @alias("naïve"),
            RESUME @alias("résumé"),
            ELITE @alias("élite"),
            FACADE @alias("façade")
        }
    },
    "FACADE"
);

test_deserializer!(
    test_unaccented_french_in_list,
    r#"["cafe", "naive", "resume"]"#,
    baml_tyannotated!([FrenchWords]),
    baml_db! {
        enum FrenchWords {
            COFFEE @alias("café"),
            NAIVE @alias("naïve"),
            RESUME @alias("résumé"),
            ELITE @alias("élite"),
            FACADE @alias("façade")
        }
    },
    ["COFFEE", "NAIVE", "RESUME"]
);

test_deserializer!(
    test_unaccented_uber_matches_accented_alias,
    r#"uber"#,
    baml_tyannotated!(GermanWords),
    baml_db! {
        enum GermanWords {
            OVER @alias("über"),
            LEADER @alias("führer"),
            DOOR @alias("tür"),
            GREEN @alias("grün")
        }
    },
    "OVER"
);

test_deserializer!(
    test_unaccented_fuhrer_matches_accented_alias,
    r#"fuhrer"#,
    baml_tyannotated!(GermanWords),
    baml_db! {
        enum GermanWords {
            OVER @alias("über"),
            LEADER @alias("führer"),
            DOOR @alias("tür"),
            GREEN @alias("grün")
        }
    },
    "LEADER"
);

test_deserializer!(
    test_unaccented_tur_matches_accented_alias,
    r#"tur"#,
    baml_tyannotated!(GermanWords),
    baml_db! {
        enum GermanWords {
            OVER @alias("über"),
            LEADER @alias("führer"),
            DOOR @alias("tür"),
            GREEN @alias("grün")
        }
    },
    "DOOR"
);

test_deserializer!(
    test_unaccented_grun_matches_accented_alias,
    r#"grun"#,
    baml_tyannotated!(GermanWords),
    baml_db! {
        enum GermanWords {
            OVER @alias("über"),
            LEADER @alias("führer"),
            DOOR @alias("tür"),
            GREEN @alias("grün")
        }
    },
    "GREEN"
);

// CLASS TESTS WITH UNACCENTED INPUT MATCHING ACCENTED ALIASES

test_deserializer!(
  test_unaccented_class_field_senor,
  r#"{"senor": "Sr. García", "nombre": "Juan", "edad": 30, "direccion": "Calle Mayor 123"}"#,
  baml_tyannotated!(SpanishForm),
  baml_db!{
      class SpanishForm {
          title: string @alias("señor"),
          name: string @alias("nombre"),
          age: int @alias("edad"),
          address: string @alias("dirección"),
      }
  },
  {
    "title": "Sr. García",
    "name": "Juan",
    "age": 30,
    "address": "Calle Mayor 123"
  }
);

test_deserializer!(
  test_mixed_accented_unaccented_class_fields,
  r#"{"señor": "Sr. García", "nombre": "Juan", "edad": 30, "direccion": "Calle Mayor 123"}"#,
  baml_tyannotated!(SpanishForm),
  baml_db!{
      class SpanishForm {
          title: string @alias("señor"),
          name: string @alias("nombre"),
          age: int @alias("edad"),
          address: string @alias("dirección"),
      }
  },
  {
    "title": "Sr. García",
    "name": "Juan",
    "age": 30,
    "address": "Calle Mayor 123"
  }
);

test_deserializer!(
  test_unaccented_french_class_fields,
  r#"{"prenom": "François", "nom": "Dupont", "ville": "Paris", "metier": "Professeur"}"#,
  baml_tyannotated!(FrenchProfile),
  baml_db!{
      class FrenchProfile {
          first_name: string @alias("prénom"),
          last_name: string @alias("nom"),
          city: string @alias("ville"),
          profession: string @alias("métier"),
      }
  },
  {
    "first_name": "François",
    "last_name": "Dupont",
    "city": "Paris",
    "profession": "Professeur"
  }
);

test_deserializer!(
  test_unaccented_portuguese_class_fields,
  r#"{"localizacao": "São Paulo", "descricao": "Uma cidade grande", "solucao": "Transporte público", "informacao": "Dados importantes"}"#,
  baml_tyannotated!(PortugueseData),
  baml_db!{
      class PortugueseData {
          location: string @alias("localização"),
          description: string @alias("descrição"),
          solution: string @alias("solução"),
          information: string @alias("informação"),
      }
  },
  {
    "location": "São Paulo",
    "description": "Uma cidade grande",
    "solution": "Transporte público",
    "information": "Dados importantes"
  }
);

// LITERAL TESTS WITH UNACCENTED INPUT

test_deserializer!(
    test_unaccented_literal_cafe,
    r#"cafe"#,
    baml_tyannotated!("café"),
    baml_db! {},
    "café"
);

test_deserializer!(
    test_unaccented_literal_resume,
    r#"resume"#,
    baml_tyannotated!("résumé"),
    baml_db! {},
    "résumé"
);

test_deserializer!(
    test_unaccented_literal_senor,
    r#"senor"#,
    baml_tyannotated!("señor"),
    baml_db! {},
    "señor"
);

test_deserializer!(
    test_unaccented_literal_in_union,
    r#"cafe"#,
    baml_tyannotated!(("café" | "résumé" | "señor")),
    baml_db! {},
    "café"
);

// =============================================================================
// CASE-INSENSITIVE UNACCENTED MATCHING TESTS
// =============================================================================

test_deserializer!(
    test_case_insensitive_unaccented_hola,
    r#"hola"#,
    baml_tyannotated!(SpanishGreeting),
    baml_db! {
        enum SpanishGreeting {
            HELLO @alias("Hola"),
            GOODBYE @alias("Adiós"),
            PLEASE @alias("Por favor"),
            THANK_YOU @alias("Gracias")
        }
    },
    "HELLO"
);

test_deserializer!(
    test_case_insensitive_unaccented_adios,
    r#"adios"#,
    baml_tyannotated!(SpanishGreeting),
    baml_db! {
        enum SpanishGreeting {
            HELLO @alias("Hola"),
            GOODBYE @alias("Adiós"),
            PLEASE @alias("Por favor"),
            THANK_YOU @alias("Gracias")
        }
    },
    "GOODBYE"
);

test_deserializer!(
    test_case_insensitive_mixed_case_unaccented,
    r#"ADIOS"#,
    baml_tyannotated!(SpanishGreeting),
    baml_db! {
        enum SpanishGreeting {
            HELLO @alias("Hola"),
            GOODBYE @alias("Adiós"),
            PLEASE @alias("Por favor"),
            THANK_YOU @alias("Gracias")
        }
    },
    "GOODBYE"
);

test_deserializer!(
    test_case_insensitive_unaccented_por_favor,
    r#"por favor"#,
    baml_tyannotated!(SpanishGreeting),
    baml_db! {
        enum SpanishGreeting {
            HELLO @alias("Hola"),
            GOODBYE @alias("Adiós"),
            PLEASE @alias("Por favor"),
            THANK_YOU @alias("Gracias")
        }
    },
    "PLEASE"
);

test_deserializer!(
    test_case_insensitive_unaccented_cafe_upper,
    r#"CAFE"#,
    baml_tyannotated!(FrenchFood),
    baml_db! {
        enum FrenchFood {
            COFFEE @alias("Café"),
            CAKE @alias("Gâteau"),
            CHEESE @alias("Fromage"),
            BREAD @alias("Pain")
        }
    },
    "COFFEE"
);

test_deserializer!(
    test_case_insensitive_unaccented_gateau,
    r#"gateau"#,
    baml_tyannotated!(FrenchFood),
    baml_db! {
        enum FrenchFood {
            COFFEE @alias("Café"),
            CAKE @alias("Gâteau"),
            CHEESE @alias("Fromage"),
            BREAD @alias("Pain")
        }
    },
    "CAKE"
);

test_deserializer!(
    test_case_insensitive_mixed_case_fromage,
    r#"FrOmAgE"#,
    baml_tyannotated!(FrenchFood),
    baml_db! {
        enum FrenchFood {
            COFFEE @alias("Café"),
            CAKE @alias("Gâteau"),
            CHEESE @alias("Fromage"),
            BREAD @alias("Pain")
        }
    },
    "CHEESE"
);

test_deserializer!(
    test_case_insensitive_french_in_sentence,
    r#"I would like some CAFE please"#,
    baml_tyannotated!(FrenchFood),
    baml_db! {
        enum FrenchFood {
            COFFEE @alias("Café"),
            CAKE @alias("Gâteau"),
            CHEESE @alias("Fromage"),
            BREAD @alias("Pain")
        }
    },
    "COFFEE"
);

// Test case insensitive unaccented matching in class fields

test_deserializer!(
  test_case_insensitive_unaccented_german_fields,
  r#"{"strasse": "Main St", "stadt": "Berlin", "uber": "Above", "grun": "Green"}"#,
  baml_tyannotated!(GermanAddress),
  baml_db!{
      class GermanAddress {
          street: string @alias("Straße"),
          city: string @alias("Stadt"),
          over: string @alias("Über"),
          green: string @alias("Grün"),
      }
  },
  {
    "street": "Main St",
    "city": "Berlin",
    "over": "Above",
    "green": "Green"
  }
);

test_deserializer!(
  test_case_insensitive_mixed_case_german_fields,
  r#"{"STRASSE": "Main St", "stadt": "Berlin", "Uber": "Above", "GRUN": "Green"}"#,
  baml_tyannotated!(GermanAddress),
  baml_db!{
      class GermanAddress {
          street: string @alias("Straße"),
          city: string @alias("Stadt"),
          over: string @alias("Über"),
          green: string @alias("Grün"),
      }
  },
  {
    "street": "Main St",
    "city": "Berlin",
    "over": "Above",
    "green": "Green"
  }
);

// Test case insensitive unaccented literals
test_deserializer!(
    test_case_insensitive_unaccented_literal_senor_upper,
    r#"SENOR"#,
    baml_tyannotated!("señor"),
    baml_db! {},
    "señor"
);

test_deserializer!(
    test_case_insensitive_unaccented_literal_resume_mixed,
    r#"ReSuMe"#,
    baml_tyannotated!("résumé"),
    baml_db! {},
    "résumé"
);

test_deserializer!(
    test_case_insensitive_unaccented_literal_naive_lower,
    r#"naive"#,
    baml_tyannotated!("Naïve"),
    baml_db! {},
    "Naïve"
);

// Test combinations of case variations and accents

test_deserializer!(
    test_complex_case_unaccented_senorita_lower,
    r#"senorita"#,
    baml_tyannotated!(ComplexAccents),
    baml_db! {
        enum ComplexAccents {
            WORD1 @alias("Señorita"),
            WORD2 @alias("CAFÉ"),
            WORD3 @alias("résumé"),
            WORD4 @alias("NAÏVE"),
            WORD5 @alias("Über")
        }
    },
    "WORD1"
);

test_deserializer!(
    test_complex_case_unaccented_cafe_lower,
    r#"cafe"#,
    baml_tyannotated!(ComplexAccents),
    baml_db! {
        enum ComplexAccents {
            WORD1 @alias("Señorita"),
            WORD2 @alias("CAFÉ"),
            WORD3 @alias("résumé"),
            WORD4 @alias("NAÏVE"),
            WORD5 @alias("Über")
        }
    },
    "WORD2"
);

test_deserializer!(
    test_complex_case_unaccented_resume_upper,
    r#"RESUME"#,
    baml_tyannotated!(ComplexAccents),
    baml_db! {
        enum ComplexAccents {
            WORD1 @alias("Señorita"),
            WORD2 @alias("CAFÉ"),
            WORD3 @alias("résumé"),
            WORD4 @alias("NAÏVE"),
            WORD5 @alias("Über")
        }
    },
    "WORD3"
);

test_deserializer!(
    test_complex_case_unaccented_naive_mixed,
    r#"NaIvE"#,
    baml_tyannotated!(ComplexAccents),
    baml_db! {
        enum ComplexAccents {
            WORD1 @alias("Señorita"),
            WORD2 @alias("CAFÉ"),
            WORD3 @alias("résumé"),
            WORD4 @alias("NAÏVE"),
            WORD5 @alias("Über")
        }
    },
    "WORD4"
);

test_deserializer!(
    test_complex_case_unaccented_uber_lower,
    r#"uber"#,
    baml_tyannotated!(ComplexAccents),
    baml_db! {
        enum ComplexAccents {
            WORD1 @alias("Señorita"),
            WORD2 @alias("CAFÉ"),
            WORD3 @alias("résumé"),
            WORD4 @alias("NAÏVE"),
            WORD5 @alias("Über")
        }
    },
    "WORD5"
);

test_deserializer!(
    test_complex_case_unaccented_list_mixed_cases,
    r#"["SENORITA", "cafe", "Resume", "naive", "UBER"]"#,
    baml_tyannotated!([ComplexAccents]),
    baml_db! {
        enum ComplexAccents {
            WORD1 @alias("Señorita"),
            WORD2 @alias("CAFÉ"),
            WORD3 @alias("résumé"),
            WORD4 @alias("NAÏVE"),
            WORD5 @alias("Über")
        }
    },
    ["WORD1", "WORD2", "WORD3", "WORD4", "WORD5"]
);

// Test case insensitive unaccented with punctuation

test_deserializer!(
    test_case_insensitive_unaccented_with_punctuation,
    r#"SENOR-JOSE"#,
    baml_tyannotated!(PunctuationAccents),
    baml_db! {
        enum PunctuationAccents {
            TEST1 @alias("señor-josé"),
            TEST2 @alias("café_bar"),
            TEST3 @alias("résumé.doc"),
            TEST4 @alias("naïve-approach")
        }
    },
    "TEST1"
);

test_deserializer!(
    test_case_insensitive_unaccented_cafe_bar,
    r#"cafe_bar"#,
    baml_tyannotated!(PunctuationAccents),
    baml_db! {
        enum PunctuationAccents {
            TEST1 @alias("señor-josé"),
            TEST2 @alias("café_bar"),
            TEST3 @alias("résumé.doc"),
            TEST4 @alias("naïve-approach")
        }
    },
    "TEST2"
);

test_deserializer!(
    test_case_insensitive_unaccented_resume_doc,
    r#"resume doc"#,
    baml_tyannotated!(PunctuationAccents),
    baml_db! {
        enum PunctuationAccents {
            TEST1 @alias("señor-josé"),
            TEST2 @alias("café_bar"),
            TEST3 @alias("résumé.doc"),
            TEST4 @alias("naïve-approach")
        }
    },
    "TEST3"
);
