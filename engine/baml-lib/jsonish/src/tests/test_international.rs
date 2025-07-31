use baml_types::{ir_type::UnionConstructor, type_meta::base::TypeMeta, LiteralValue};

use super::*;

// =============================================================================
// ENUM TESTS WITH ACCENTED ALIASES
// =============================================================================

const CUISINE_ENUM_WITH_ACCENTED_ALIASES: &str = r#"
enum CuisineType {
  FRENCH @alias("française")
  SPANISH @alias("española")
  PORTUGUESE @alias("portuguesa")
  ITALIAN @alias("italiana")
  CHINESE @alias("中式")
  ARABIC @alias("العربية")
  RUSSIAN @alias("русская")
  JAPANESE @alias("日本料理")
}
"#;

test_deserializer!(
    test_accented_alias_french,
    CUISINE_ENUM_WITH_ACCENTED_ALIASES,
    r#"française"#,
    TypeIR::r#enum("CuisineType"),
    "FRENCH"
);

test_deserializer!(
    test_accented_alias_spanish,
    CUISINE_ENUM_WITH_ACCENTED_ALIASES,
    r#"española"#,
    TypeIR::r#enum("CuisineType"),
    "SPANISH"
);

test_deserializer!(
    test_accented_alias_chinese,
    CUISINE_ENUM_WITH_ACCENTED_ALIASES,
    r#"中式"#,
    TypeIR::r#enum("CuisineType"),
    "CHINESE"
);

test_deserializer!(
    test_accented_alias_arabic,
    CUISINE_ENUM_WITH_ACCENTED_ALIASES,
    r#"العربية"#,
    TypeIR::r#enum("CuisineType"),
    "ARABIC"
);

test_deserializer!(
    test_accented_alias_russian,
    CUISINE_ENUM_WITH_ACCENTED_ALIASES,
    r#"русская"#,
    TypeIR::r#enum("CuisineType"),
    "RUSSIAN"
);

test_deserializer!(
    test_accented_alias_japanese,
    CUISINE_ENUM_WITH_ACCENTED_ALIASES,
    r#"日本料理"#,
    TypeIR::r#enum("CuisineType"),
    "JAPANESE"
);

test_failing_deserializer!(
    test_original_enum_values_fail_when_aliases_exist,
    CUISINE_ENUM_WITH_ACCENTED_ALIASES,
    r#"FRENCH"#,
    TypeIR::r#enum("CuisineType")
);

test_deserializer!(
    test_accented_alias_in_sentence,
    CUISINE_ENUM_WITH_ACCENTED_ALIASES,
    r#"The restaurant serves española cuisine with authentic flavors"#,
    TypeIR::r#enum("CuisineType"),
    "SPANISH"
);

test_deserializer!(
    test_accented_alias_case_variations,
    CUISINE_ENUM_WITH_ACCENTED_ALIASES,
    r#"Française"#,
    TypeIR::r#enum("CuisineType"),
    "FRENCH"
);

test_deserializer!(
    test_accented_alias_list_mixed_scripts,
    CUISINE_ENUM_WITH_ACCENTED_ALIASES,
    r#"["française", "中式", "العربية"]"#,
    TypeIR::list(TypeIR::r#enum("CuisineType")),
    ["FRENCH", "CHINESE", "ARABIC"]
);

const DOCUMENT_ENUM_WITH_FRENCH_ALIASES: &str = r#"
enum DocumentType {
  INVOICE @alias("facture")
  RECEIPT @alias("reçu")
  CONTRACT @alias("contrat")
  REPORT @alias("rapport")
  LETTER @alias("lettre")
}
"#;

test_deserializer!(
    test_french_alias_invoice,
    DOCUMENT_ENUM_WITH_FRENCH_ALIASES,
    r#"facture"#,
    TypeIR::r#enum("DocumentType"),
    "INVOICE"
);

test_deserializer!(
    test_french_alias_receipt_with_accent,
    DOCUMENT_ENUM_WITH_FRENCH_ALIASES,
    r#"reçu"#,
    TypeIR::r#enum("DocumentType"),
    "RECEIPT"
);

test_failing_deserializer!(
    test_original_enum_values_fail_with_french_aliases,
    DOCUMENT_ENUM_WITH_FRENCH_ALIASES,
    r#"INVOICE"#,
    TypeIR::r#enum("DocumentType")
);

test_deserializer!(
    test_french_alias_in_context,
    DOCUMENT_ENUM_WITH_FRENCH_ALIASES,
    r#"Please process this facture document"#,
    TypeIR::r#enum("DocumentType"),
    "INVOICE"
);

const STATUS_ENUM_WITH_ACCENTED_ALIASES: &str = r#"
enum Status {
  ACTIVE @alias("actif")
  INACTIVE @alias("inactif")
  PENDING @alias("en_attente")
  COMPLETED @alias("terminé")
  CANCELLED @alias("annulé")
}
"#;

test_deserializer!(
    test_status_french_alias_active,
    STATUS_ENUM_WITH_ACCENTED_ALIASES,
    r#"actif"#,
    TypeIR::r#enum("Status"),
    "ACTIVE"
);

test_deserializer!(
    test_status_french_alias_completed,
    STATUS_ENUM_WITH_ACCENTED_ALIASES,
    r#"terminé"#,
    TypeIR::r#enum("Status"),
    "COMPLETED"
);

test_deserializer!(
    test_status_french_alias_cancelled,
    STATUS_ENUM_WITH_ACCENTED_ALIASES,
    r#"annulé"#,
    TypeIR::r#enum("Status"),
    "CANCELLED"
);

const PRIORITY_ENUM_WITH_MULTILINGUAL_ALIASES: &str = r#"
enum Priority {
  HIGH @alias("élevé")
  MEDIUM @alias("médium")
  LOW @alias("baixo")
  URGENT @alias("紧急")
  NORMAL @alias("عادي")
}
"#;

test_deserializer!(
    test_priority_french_high,
    PRIORITY_ENUM_WITH_MULTILINGUAL_ALIASES,
    r#"élevé"#,
    TypeIR::r#enum("Priority"),
    "HIGH"
);

test_deserializer!(
    test_priority_french_medium,
    PRIORITY_ENUM_WITH_MULTILINGUAL_ALIASES,
    r#"médium"#,
    TypeIR::r#enum("Priority"),
    "MEDIUM"
);

test_deserializer!(
    test_priority_portuguese_low,
    PRIORITY_ENUM_WITH_MULTILINGUAL_ALIASES,
    r#"baixo"#,
    TypeIR::r#enum("Priority"),
    "LOW"
);

test_deserializer!(
    test_priority_chinese_urgent,
    PRIORITY_ENUM_WITH_MULTILINGUAL_ALIASES,
    r#"紧急"#,
    TypeIR::r#enum("Priority"),
    "URGENT"
);

test_deserializer!(
    test_priority_arabic_normal,
    PRIORITY_ENUM_WITH_MULTILINGUAL_ALIASES,
    r#"عادي"#,
    TypeIR::r#enum("Priority"),
    "NORMAL"
);

test_failing_deserializer!(
    test_original_priority_values_fail_with_aliases,
    PRIORITY_ENUM_WITH_MULTILINGUAL_ALIASES,
    r#"HIGH"#,
    TypeIR::r#enum("Priority")
);

// original values are not allowed in lists
// so we should return an empty list
test_deserializer!(
    test_multiple_original_enum_values_fail,
    PRIORITY_ENUM_WITH_MULTILINGUAL_ALIASES,
    r#"["HIGH", "MEDIUM", "LOW"]"#,
    TypeIR::list(TypeIR::r#enum("Priority")),
    // since "médium" -> "MEDIUM"
    ["MEDIUM"]
);

// =============================================================================
// LITERAL TESTS WITH ACCENTED VALUES
// =============================================================================

test_deserializer!(
    test_literal_string_french_name,
    EMPTY_FILE,
    r#"François"#,
    TypeIR::Literal(LiteralValue::String("François".into()), TypeMeta::default()),
    "François"
);

test_deserializer!(
    test_literal_string_spanish_greeting,
    EMPTY_FILE,
    r#"¡Hola!"#,
    TypeIR::Literal(LiteralValue::String("¡Hola!".into()), TypeMeta::default()),
    "¡Hola!"
);

test_deserializer!(
    test_literal_string_portuguese_word,
    EMPTY_FILE,
    r#"São Paulo"#,
    TypeIR::Literal(
        LiteralValue::String("São Paulo".into()),
        TypeMeta::default()
    ),
    "São Paulo"
);

test_deserializer!(
    test_literal_string_german_umlaut,
    EMPTY_FILE,
    r#"Müller"#,
    TypeIR::Literal(LiteralValue::String("Müller".into()), TypeMeta::default()),
    "Müller"
);

test_deserializer!(
    test_literal_string_chinese_characters,
    EMPTY_FILE,
    r#"北京"#,
    TypeIR::Literal(LiteralValue::String("北京".into()), TypeMeta::default()),
    "北京"
);

test_deserializer!(
    test_literal_string_arabic_text,
    EMPTY_FILE,
    r#"السلام عليكم"#,
    TypeIR::Literal(
        LiteralValue::String("السلام عليكم".into()),
        TypeMeta::default()
    ),
    "السلام عليكم"
);

test_deserializer!(
    test_literal_string_russian_cyrillic,
    EMPTY_FILE,
    r#"Москва"#,
    TypeIR::Literal(LiteralValue::String("Москва".into()), TypeMeta::default()),
    "Москва"
);

test_deserializer!(
    test_literal_string_japanese_hiragana,
    EMPTY_FILE,
    r#"こんにちは"#,
    TypeIR::Literal(
        LiteralValue::String("こんにちは".into()),
        TypeMeta::default()
    ),
    "こんにちは"
);

test_deserializer!(
    test_literal_string_accented_with_quotes,
    EMPTY_FILE,
    r#""François""#,
    TypeIR::Literal(LiteralValue::String("François".into()), TypeMeta::default()),
    "François"
);

test_deserializer!(
    test_literal_string_accented_case_insensitive,
    EMPTY_FILE,
    r#"françois"#,
    TypeIR::Literal(LiteralValue::String("François".into()), TypeMeta::default()),
    "François"
);

test_deserializer!(
    test_literal_string_accented_in_sentence,
    EMPTY_FILE,
    r#"The name is François for this person"#,
    TypeIR::Literal(LiteralValue::String("François".into()), TypeMeta::default()),
    "François"
);

test_deserializer!(
    test_literal_string_cafe_with_emoji,
    EMPTY_FILE,
    r#"Café ☕"#,
    TypeIR::Literal(LiteralValue::String("Café ☕".into()), TypeMeta::default()),
    "Café ☕"
);

test_deserializer!(
    test_union_literal_city_names,
    EMPTY_FILE,
    r#"São Paulo"#,
    TypeIR::union(vec![
        TypeIR::Literal(LiteralValue::String("Paris".into()), TypeMeta::default()),
        TypeIR::Literal(
            LiteralValue::String("São Paulo".into()),
            TypeMeta::default()
        ),
        TypeIR::Literal(LiteralValue::String("Zürich".into()), TypeMeta::default()),
    ]),
    "São Paulo"
);

test_deserializer!(
    test_union_literal_mixed_languages,
    EMPTY_FILE,
    r#"北京"#,
    TypeIR::union(vec![
        TypeIR::Literal(LiteralValue::String("Paris".into()), TypeMeta::default()),
        TypeIR::Literal(LiteralValue::String("北京".into()), TypeMeta::default()),
        TypeIR::Literal(LiteralValue::String("القاهرة".into()), TypeMeta::default()),
    ]),
    "北京"
);

test_deserializer!(
    test_literal_string_diacritics_combination,
    EMPTY_FILE,
    r#"naïve résumé"#,
    TypeIR::Literal(
        LiteralValue::String("naïve résumé".into()),
        TypeMeta::default()
    ),
    "naïve résumé"
);

// =============================================================================
// CLASS TESTS WITH ACCENTED ALIASES
// =============================================================================

const RESTAURANT_CLASS_WITH_FRENCH_ALIASES: &str = r#"
class Restaurant {
  name string @alias("nom")
  address string @alias("adresse")
  specialty string @alias("spécialité")
  stars int @alias("étoiles")
}
"#;

test_deserializer!(
  test_french_field_aliases,
  RESTAURANT_CLASS_WITH_FRENCH_ALIASES,
  r#"{"nom": "Le Petit Café", "adresse": "Champs-Élysées", "spécialité": "crêpes bretonnes", "étoiles": 4}"#,
  TypeIR::class("Restaurant"),
  {
    "name": "Le Petit Café",
    "address": "Champs-Élysées",
    "specialty": "crêpes bretonnes",
    "stars": 4
  }
);

test_deserializer!(
  test_french_field_aliases_without_quotes,
  RESTAURANT_CLASS_WITH_FRENCH_ALIASES,
  r#"{nom: "Le Petit Café", adresse: Champs-Élysées, spécialité: "crêpes bretonnes", étoiles: 4}"#,
  TypeIR::class("Restaurant"),
  {
    "name": "Le Petit Café",
    "address": "Champs-Élysées",
    "specialty": "crêpes bretonnes",
    "stars": 4
  }
);

test_failing_deserializer!(
    test_original_field_names_fail_when_aliases_exist,
    RESTAURANT_CLASS_WITH_FRENCH_ALIASES,
    r#"{"name": "Le Petit Café", "address": "Champs-Élysées", "specialty": "crêpes bretonnes", "stars": 4}"#,
    TypeIR::class("Restaurant")
);

const INTERNATIONAL_CONTACT_WITH_ALIASES: &str = r#"
class InternationalContact {
  first_name string @alias("prénom")
  family_name string @alias("família")
  city string @alias("città")
  street string @alias("straße")
  field string @alias("поле")
  data_field string @alias("フィールド")
}
"#;

test_deserializer!(
  test_international_field_aliases,
  INTERNATIONAL_CONTACT_WITH_ALIASES,
  r#"{"prénom": "François", "família": "Silva", "città": "Milano", "straße": "Hauptstraße", "поле": "значение", "フィールド": "値"}"#,
  TypeIR::class("InternationalContact"),
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
  INTERNATIONAL_CONTACT_WITH_ALIASES,
  r#"Here is the contact information:
  {
    "prénom": "José",
    "família": "González", 
    "città": "Barcelona",
    "straße": "Königstraße",
    "поле": "текст",
    "フィールド": "データ"
  }"#,
  TypeIR::class("InternationalContact"),
  {
    "first_name": "José",
    "family_name": "González",
    "city": "Barcelona",
    "street": "Königstraße",
    "field": "текст",
    "data_field": "データ"
  }
);

const PERSON_WITH_FRENCH_NESTED_ALIASES: &str = r#"
class Address {
  number int @alias("numéro")
  street string @alias("rue")
  city string @alias("ville")
  region string @alias("région")
}

class Person {
  first_name string @alias("prénom")
  last_name string @alias("nom")
  age int @alias("âge")
  address Address @alias("adresse")
}
"#;

test_deserializer!(
  test_french_nested_class_aliases,
  PERSON_WITH_FRENCH_NESTED_ALIASES,
  r#"{"prénom": "François", "nom": "Müller", "âge": 35, "adresse": {"numéro": 42, "rue": "Champs-Élysées", "ville": "Paris", "région": "Île-de-France"}}"#,
  TypeIR::class("Person"),
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

const CLASS_WITH_ACCENTED_ALIASES: &str = r#"
class ProductInfo {
  name string @alias("nom")
  price float @alias("prix")
  category string @alias("catégorie")
  description string @alias("description")
}
"#;

test_deserializer!(
  test_class_with_accented_aliases,
  CLASS_WITH_ACCENTED_ALIASES,
  r#"{"nom": "Café Latte", "prix": 4.50, "catégorie": "Boissons", "description": "Délicieux café"}"#,
  TypeIR::class("ProductInfo"),
  {
    "name": "Café Latte",
    "price": 4.50,
    "category": "Boissons",
    "description": "Délicieux café"
  }
);

test_failing_deserializer!(
    test_original_field_names_fail_with_aliases,
    CLASS_WITH_ACCENTED_ALIASES,
    r#"{"name": "Café Latte", "price": 4.50, "category": "Boissons", "description": "Délicieux café"}"#,
    TypeIR::class("ProductInfo")
);

test_deserializer!(
  test_library_with_french_aliases,
  r#"
  class Library {
    books string[] @alias("livres")
    authors string[] @alias("auteurs")
    years int[] @alias("années")
  }
  "#,
  r#"{"livres": ["L'Étranger", "Amélie Poulain", "Naïveté"], "auteurs": ["Camus", "Jeunet", "Müller"], "années": [1942, 2001, 2020]}"#,
  TypeIR::class("Library"),
  {
    "books": ["L'Étranger", "Amélie Poulain", "Naïveté"],
    "authors": ["Camus", "Jeunet", "Müller"],
    "years": [1942, 2001, 2020]
  }
);

// =============================================================================
// UNACCENTED INPUT MATCHING ACCENTED ALIASES
// =============================================================================

const SPANISH_ENUM_WITH_ACCENTED_ALIASES: &str = r#"
enum SpanishTitle {
  MISTER @alias("señor")
  MISS @alias("señorita")
  DOCTOR @alias("doctor")
  PROFESSOR @alias("profesor")
}
"#;

test_deserializer!(
    test_unaccented_senor_matches_accented_alias,
    SPANISH_ENUM_WITH_ACCENTED_ALIASES,
    r#"senor"#,
    TypeIR::r#enum("SpanishTitle"),
    "MISTER"
);

test_deserializer!(
    test_unaccented_senorita_matches_accented_alias,
    SPANISH_ENUM_WITH_ACCENTED_ALIASES,
    r#"senorita"#,
    TypeIR::r#enum("SpanishTitle"),
    "MISS"
);

test_deserializer!(
    test_unaccented_profesor_matches_accented_alias,
    SPANISH_ENUM_WITH_ACCENTED_ALIASES,
    r#"profesor"#,
    TypeIR::r#enum("SpanishTitle"),
    "PROFESSOR"
);

test_deserializer!(
    test_unaccented_in_sentence,
    SPANISH_ENUM_WITH_ACCENTED_ALIASES,
    r#"The title is senor for this person"#,
    TypeIR::r#enum("SpanishTitle"),
    "MISTER"
);

const FRENCH_ENUM_WITH_ACCENTED_ALIASES: &str = r#"
enum FrenchWords {
  COFFEE @alias("café")
  NAIVE @alias("naïve")
  RESUME @alias("résumé")
  ELITE @alias("élite")
  FACADE @alias("façade")
}
"#;

test_deserializer!(
    test_unaccented_cafe_matches_accented_alias,
    FRENCH_ENUM_WITH_ACCENTED_ALIASES,
    r#"cafe"#,
    TypeIR::r#enum("FrenchWords"),
    "COFFEE"
);

test_deserializer!(
    test_unaccented_naive_matches_accented_alias,
    FRENCH_ENUM_WITH_ACCENTED_ALIASES,
    r#"naive"#,
    TypeIR::r#enum("FrenchWords"),
    "NAIVE"
);

test_deserializer!(
    test_unaccented_resume_matches_accented_alias,
    FRENCH_ENUM_WITH_ACCENTED_ALIASES,
    r#"resume"#,
    TypeIR::r#enum("FrenchWords"),
    "RESUME"
);

test_deserializer!(
    test_unaccented_elite_matches_accented_alias,
    FRENCH_ENUM_WITH_ACCENTED_ALIASES,
    r#"elite"#,
    TypeIR::r#enum("FrenchWords"),
    "ELITE"
);

test_deserializer!(
    test_unaccented_facade_matches_accented_alias,
    FRENCH_ENUM_WITH_ACCENTED_ALIASES,
    r#"facade"#,
    TypeIR::r#enum("FrenchWords"),
    "FACADE"
);

test_deserializer!(
    test_unaccented_french_in_list,
    FRENCH_ENUM_WITH_ACCENTED_ALIASES,
    r#"["cafe", "naive", "resume"]"#,
    TypeIR::list(TypeIR::r#enum("FrenchWords")),
    ["COFFEE", "NAIVE", "RESUME"]
);

const GERMAN_ENUM_WITH_ACCENTED_ALIASES: &str = r#"
enum GermanWords {
  OVER @alias("über")
  LEADER @alias("führer")
  DOOR @alias("tür")
  GREEN @alias("grün")
}
"#;

test_deserializer!(
    test_unaccented_uber_matches_accented_alias,
    GERMAN_ENUM_WITH_ACCENTED_ALIASES,
    r#"uber"#,
    TypeIR::r#enum("GermanWords"),
    "OVER"
);

test_deserializer!(
    test_unaccented_fuhrer_matches_accented_alias,
    GERMAN_ENUM_WITH_ACCENTED_ALIASES,
    r#"fuhrer"#,
    TypeIR::r#enum("GermanWords"),
    "LEADER"
);

test_deserializer!(
    test_unaccented_tur_matches_accented_alias,
    GERMAN_ENUM_WITH_ACCENTED_ALIASES,
    r#"tur"#,
    TypeIR::r#enum("GermanWords"),
    "DOOR"
);

test_deserializer!(
    test_unaccented_grun_matches_accented_alias,
    GERMAN_ENUM_WITH_ACCENTED_ALIASES,
    r#"grun"#,
    TypeIR::r#enum("GermanWords"),
    "GREEN"
);

// CLASS TESTS WITH UNACCENTED INPUT MATCHING ACCENTED ALIASES

const SPANISH_CLASS_WITH_ACCENTED_ALIASES: &str = r#"
class SpanishForm {
  title string @alias("señor")
  name string @alias("nombre")
  age int @alias("edad")
  address string @alias("dirección")
}
"#;

test_deserializer!(
  test_unaccented_class_field_senor,
  SPANISH_CLASS_WITH_ACCENTED_ALIASES,
  r#"{"senor": "Sr. García", "nombre": "Juan", "edad": 30, "direccion": "Calle Mayor 123"}"#,
  TypeIR::class("SpanishForm"),
  {
    "title": "Sr. García",
    "name": "Juan",
    "age": 30,
    "address": "Calle Mayor 123"
  }
);

test_deserializer!(
  test_mixed_accented_unaccented_class_fields,
  SPANISH_CLASS_WITH_ACCENTED_ALIASES,
  r#"{"señor": "Sr. García", "nombre": "Juan", "edad": 30, "direccion": "Calle Mayor 123"}"#,
  TypeIR::class("SpanishForm"),
  {
    "title": "Sr. García",
    "name": "Juan",
    "age": 30,
    "address": "Calle Mayor 123"
  }
);

const FRENCH_CLASS_WITH_ACCENTED_ALIASES: &str = r#"
class FrenchProfile {
  first_name string @alias("prénom")
  last_name string @alias("nom")
  city string @alias("ville")
  profession string @alias("métier")
}
"#;

test_deserializer!(
  test_unaccented_french_class_fields,
  FRENCH_CLASS_WITH_ACCENTED_ALIASES,
  r#"{"prenom": "François", "nom": "Dupont", "ville": "Paris", "metier": "Professeur"}"#,
  TypeIR::class("FrenchProfile"),
  {
    "first_name": "François",
    "last_name": "Dupont",
    "city": "Paris",
    "profession": "Professeur"
  }
);

const PORTUGUESE_CLASS_WITH_ACCENTED_ALIASES: &str = r#"
class PortugueseData {
  location string @alias("localização")
  description string @alias("descrição")
  solution string @alias("solução")
  information string @alias("informação")
}
"#;

test_deserializer!(
  test_unaccented_portuguese_class_fields,
  PORTUGUESE_CLASS_WITH_ACCENTED_ALIASES,
  r#"{"localizacao": "São Paulo", "descricao": "Uma cidade grande", "solucao": "Transporte público", "informacao": "Dados importantes"}"#,
  TypeIR::class("PortugueseData"),
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
    EMPTY_FILE,
    r#"cafe"#,
    TypeIR::Literal(LiteralValue::String("café".into()), TypeMeta::default()),
    "café"
);

test_deserializer!(
    test_unaccented_literal_resume,
    EMPTY_FILE,
    r#"resume"#,
    TypeIR::Literal(LiteralValue::String("résumé".into()), TypeMeta::default()),
    "résumé"
);

test_deserializer!(
    test_unaccented_literal_senor,
    EMPTY_FILE,
    r#"senor"#,
    TypeIR::Literal(LiteralValue::String("señor".into()), TypeMeta::default()),
    "señor"
);

test_deserializer!(
    test_unaccented_literal_in_union,
    EMPTY_FILE,
    r#"cafe"#,
    TypeIR::union(vec![
        TypeIR::Literal(LiteralValue::String("café".into()), TypeMeta::default()),
        TypeIR::Literal(LiteralValue::String("résumé".into()), TypeMeta::default()),
        TypeIR::Literal(LiteralValue::String("señor".into()), TypeMeta::default()),
    ]),
    "café"
);

// =============================================================================
// CASE-INSENSITIVE UNACCENTED MATCHING TESTS
// =============================================================================

const MIXED_CASE_SPANISH_ENUM: &str = r#"
enum SpanishGreeting {
  HELLO @alias("Hola")
  GOODBYE @alias("Adiós")
  PLEASE @alias("Por favor")
  THANK_YOU @alias("Gracias")
}
"#;

test_deserializer!(
    test_case_insensitive_unaccented_hola,
    MIXED_CASE_SPANISH_ENUM,
    r#"hola"#,
    TypeIR::r#enum("SpanishGreeting"),
    "HELLO"
);

test_deserializer!(
    test_case_insensitive_unaccented_adios,
    MIXED_CASE_SPANISH_ENUM,
    r#"adios"#,
    TypeIR::r#enum("SpanishGreeting"),
    "GOODBYE"
);

test_deserializer!(
    test_case_insensitive_mixed_case_unaccented,
    MIXED_CASE_SPANISH_ENUM,
    r#"ADIOS"#,
    TypeIR::r#enum("SpanishGreeting"),
    "GOODBYE"
);

test_deserializer!(
    test_case_insensitive_unaccented_por_favor,
    MIXED_CASE_SPANISH_ENUM,
    r#"por favor"#,
    TypeIR::r#enum("SpanishGreeting"),
    "PLEASE"
);

const MIXED_CASE_FRENCH_ENUM: &str = r#"
enum FrenchFood {
  COFFEE @alias("Café")
  CAKE @alias("Gâteau")
  CHEESE @alias("Fromage")
  BREAD @alias("Pain")
}
"#;

test_deserializer!(
    test_case_insensitive_unaccented_cafe_upper,
    MIXED_CASE_FRENCH_ENUM,
    r#"CAFE"#,
    TypeIR::r#enum("FrenchFood"),
    "COFFEE"
);

test_deserializer!(
    test_case_insensitive_unaccented_gateau,
    MIXED_CASE_FRENCH_ENUM,
    r#"gateau"#,
    TypeIR::r#enum("FrenchFood"),
    "CAKE"
);

test_deserializer!(
    test_case_insensitive_mixed_case_fromage,
    MIXED_CASE_FRENCH_ENUM,
    r#"FrOmAgE"#,
    TypeIR::r#enum("FrenchFood"),
    "CHEESE"
);

test_deserializer!(
    test_case_insensitive_french_in_sentence,
    MIXED_CASE_FRENCH_ENUM,
    r#"I would like some CAFE please"#,
    TypeIR::r#enum("FrenchFood"),
    "COFFEE"
);

// Test case insensitive unaccented matching in class fields
const CASE_MIXED_GERMAN_CLASS: &str = r#"
class GermanAddress {
  street string @alias("Straße")
  city string @alias("Stadt")
  over string @alias("Über")
  green string @alias("Grün")
}
"#;

test_deserializer!(
  test_case_insensitive_unaccented_german_fields,
  CASE_MIXED_GERMAN_CLASS,
  r#"{"strasse": "Main St", "stadt": "Berlin", "uber": "Above", "grun": "Green"}"#,
  TypeIR::class("GermanAddress"),
  {
    "street": "Main St",
    "city": "Berlin",
    "over": "Above",
    "green": "Green"
  }
);

test_deserializer!(
  test_case_insensitive_mixed_case_german_fields,
  CASE_MIXED_GERMAN_CLASS,
  r#"{"STRASSE": "Main St", "stadt": "Berlin", "Uber": "Above", "GRUN": "Green"}"#,
  TypeIR::class("GermanAddress"),
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
    EMPTY_FILE,
    r#"SENOR"#,
    TypeIR::Literal(LiteralValue::String("señor".into()), TypeMeta::default()),
    "señor"
);

test_deserializer!(
    test_case_insensitive_unaccented_literal_resume_mixed,
    EMPTY_FILE,
    r#"ReSuMe"#,
    TypeIR::Literal(LiteralValue::String("résumé".into()), TypeMeta::default()),
    "résumé"
);

test_deserializer!(
    test_case_insensitive_unaccented_literal_naive_lower,
    EMPTY_FILE,
    r#"naive"#,
    TypeIR::Literal(LiteralValue::String("Naïve".into()), TypeMeta::default()),
    "Naïve"
);

// Test combinations of case variations and accents
const COMPLEX_ACCENT_ENUM: &str = r#"
enum ComplexAccents {
  WORD1 @alias("Señorita")
  WORD2 @alias("CAFÉ")
  WORD3 @alias("résumé")
  WORD4 @alias("NAÏVE")
  WORD5 @alias("Über")
}
"#;

test_deserializer!(
    test_complex_case_unaccented_senorita_lower,
    COMPLEX_ACCENT_ENUM,
    r#"senorita"#,
    TypeIR::r#enum("ComplexAccents"),
    "WORD1"
);

test_deserializer!(
    test_complex_case_unaccented_cafe_lower,
    COMPLEX_ACCENT_ENUM,
    r#"cafe"#,
    TypeIR::r#enum("ComplexAccents"),
    "WORD2"
);

test_deserializer!(
    test_complex_case_unaccented_resume_upper,
    COMPLEX_ACCENT_ENUM,
    r#"RESUME"#,
    TypeIR::r#enum("ComplexAccents"),
    "WORD3"
);

test_deserializer!(
    test_complex_case_unaccented_naive_mixed,
    COMPLEX_ACCENT_ENUM,
    r#"NaIvE"#,
    TypeIR::r#enum("ComplexAccents"),
    "WORD4"
);

test_deserializer!(
    test_complex_case_unaccented_uber_lower,
    COMPLEX_ACCENT_ENUM,
    r#"uber"#,
    TypeIR::r#enum("ComplexAccents"),
    "WORD5"
);

test_deserializer!(
    test_complex_case_unaccented_list_mixed_cases,
    COMPLEX_ACCENT_ENUM,
    r#"["SENORITA", "cafe", "Resume", "naive", "UBER"]"#,
    TypeIR::list(TypeIR::r#enum("ComplexAccents")),
    ["WORD1", "WORD2", "WORD3", "WORD4", "WORD5"]
);

// Test case insensitive unaccented with punctuation
const PUNCTUATION_ACCENT_ENUM: &str = r#"
enum PunctuationAccents {
  TEST1 @alias("señor-josé")
  TEST2 @alias("café_bar")
  TEST3 @alias("résumé.doc")
  TEST4 @alias("naïve-approach")
}
"#;

test_deserializer!(
    test_case_insensitive_unaccented_with_punctuation,
    PUNCTUATION_ACCENT_ENUM,
    r#"SENOR-JOSE"#,
    TypeIR::r#enum("PunctuationAccents"),
    "TEST1"
);

test_deserializer!(
    test_case_insensitive_unaccented_cafe_bar,
    PUNCTUATION_ACCENT_ENUM,
    r#"cafe_bar"#,
    TypeIR::r#enum("PunctuationAccents"),
    "TEST2"
);

test_deserializer!(
    test_case_insensitive_unaccented_resume_doc,
    PUNCTUATION_ACCENT_ENUM,
    r#"resume doc"#,
    TypeIR::r#enum("PunctuationAccents"),
    "TEST3"
);
