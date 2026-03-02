use super::*;

// =============================================================================
// ENUM TESTS WITH ACCENTED ALIASES
// =============================================================================

fn cuisine_enum() -> TyResolved<'static, &'static str> {
    enum_ty(
        "CuisineType",
        vec![
            variant_with_aliases("FRENCH", vec!["française"]),
            variant_with_aliases("SPANISH", vec!["española"]),
            variant_with_aliases("PORTUGUESE", vec!["portuguesa"]),
            variant_with_aliases("ITALIAN", vec!["italiana"]),
            variant_with_aliases("CHINESE", vec!["中式"]),
            variant_with_aliases("ARABIC", vec!["العربية"]),
            variant_with_aliases("RUSSIAN", vec!["русская"]),
            variant_with_aliases("JAPANESE", vec!["日本料理"]),
        ],
    )
}

test_deserializer!(
    test_accented_alias_french,
    r#"française"#,
    cuisine_enum(),
    empty_db(),
    "FRENCH"
);

test_deserializer!(
    test_accented_alias_spanish,
    r#"española"#,
    cuisine_enum(),
    empty_db(),
    "SPANISH"
);

test_deserializer!(
    test_accented_alias_chinese,
    r#"中式"#,
    cuisine_enum(),
    empty_db(),
    "CHINESE"
);

test_deserializer!(
    test_accented_alias_arabic,
    r#"العربية"#,
    cuisine_enum(),
    empty_db(),
    "ARABIC"
);

test_deserializer!(
    test_accented_alias_russian,
    r#"русская"#,
    cuisine_enum(),
    empty_db(),
    "RUSSIAN"
);

test_deserializer!(
    test_accented_alias_japanese,
    r#"日本料理"#,
    cuisine_enum(),
    empty_db(),
    "JAPANESE"
);

test_failing_deserializer!(
    test_original_enum_values_fail_when_aliases_exist,
    r#"FRENCH"#,
    cuisine_enum(),
    empty_db()
);

test_deserializer!(
    test_accented_alias_in_sentence,
    r#"The restaurant serves española cuisine with authentic flavors"#,
    cuisine_enum(),
    empty_db(),
    "SPANISH"
);

test_deserializer!(
    test_accented_alias_case_variations,
    r#"Française"#,
    cuisine_enum(),
    empty_db(),
    "FRENCH"
);

#[test]
fn test_accented_alias_list_mixed_scripts() {
    let cuisine = cuisine_enum();
    let mut db = TypeRefDb::new();
    db.try_add("CuisineType", cuisine).ok().unwrap();
    let target_ty = array_of(annotated(Ty::Unresolved("CuisineType")));

    let raw = r#"["française", "中式", "العربية"]"#;
    let parsed =
        crate::jsonish::parse(raw, Default::default(), true).expect("jsonish::parse failed");
    let ctx = crate::deserializer::coercer::ParsingContext::new(target_ty.as_ref(), &db);
    let annots = TypeAnnotations::default();
    let target = TyWithMeta::new(target_ty.as_ref(), &annots);
    let result = TyResolvedRef::coerce(&ctx, target, &parsed);
    assert!(result.is_ok(), "Failed to parse: {:?}", result);
    let value = result.unwrap().unwrap();
    let json_value = serde_json::to_value(&value).unwrap();
    let expected = serde_json::json!(["FRENCH", "CHINESE", "ARABIC"]);
    assert_eq!(json_value, expected);
}

fn document_enum() -> TyResolved<'static, &'static str> {
    enum_ty(
        "DocumentType",
        vec![
            variant_with_aliases("INVOICE", vec!["facture"]),
            variant_with_aliases("RECEIPT", vec!["reçu"]),
            variant_with_aliases("CONTRACT", vec!["contrat"]),
            variant_with_aliases("REPORT", vec!["rapport"]),
            variant_with_aliases("LETTER", vec!["lettre"]),
        ],
    )
}

test_deserializer!(
    test_french_alias_invoice,
    r#"facture"#,
    document_enum(),
    empty_db(),
    "INVOICE"
);

test_deserializer!(
    test_french_alias_receipt_with_accent,
    r#"reçu"#,
    document_enum(),
    empty_db(),
    "RECEIPT"
);

test_failing_deserializer!(
    test_original_enum_values_fail_with_french_aliases,
    r#"INVOICE"#,
    document_enum(),
    empty_db()
);

test_deserializer!(
    test_french_alias_in_context,
    r#"Please process this facture document"#,
    document_enum(),
    empty_db(),
    "INVOICE"
);

fn status_enum() -> TyResolved<'static, &'static str> {
    enum_ty(
        "Status",
        vec![
            variant_with_aliases("ACTIVE", vec!["actif"]),
            variant_with_aliases("INACTIVE", vec!["inactif"]),
            variant_with_aliases("PENDING", vec!["en_attente"]),
            variant_with_aliases("COMPLETED", vec!["terminé"]),
            variant_with_aliases("CANCELLED", vec!["annulé"]),
        ],
    )
}

test_deserializer!(
    test_status_french_alias_active,
    r#"actif"#,
    status_enum(),
    empty_db(),
    "ACTIVE"
);

test_deserializer!(
    test_status_french_alias_completed,
    r#"terminé"#,
    status_enum(),
    empty_db(),
    "COMPLETED"
);

test_deserializer!(
    test_status_french_alias_cancelled,
    r#"annulé"#,
    status_enum(),
    empty_db(),
    "CANCELLED"
);

fn priority_enum() -> TyResolved<'static, &'static str> {
    enum_ty(
        "Priority",
        vec![
            variant_with_aliases("HIGH", vec!["élevé"]),
            variant_with_aliases("MEDIUM", vec!["médium"]),
            variant_with_aliases("LOW", vec!["baixo"]),
            variant_with_aliases("URGENT", vec!["紧急"]),
            variant_with_aliases("NORMAL", vec!["عادي"]),
        ],
    )
}

test_deserializer!(
    test_priority_french_high,
    r#"élevé"#,
    priority_enum(),
    empty_db(),
    "HIGH"
);

test_deserializer!(
    test_priority_french_medium,
    r#"médium"#,
    priority_enum(),
    empty_db(),
    "MEDIUM"
);

test_deserializer!(
    test_priority_portuguese_low,
    r#"baixo"#,
    priority_enum(),
    empty_db(),
    "LOW"
);

test_deserializer!(
    test_priority_chinese_urgent,
    r#"紧急"#,
    priority_enum(),
    empty_db(),
    "URGENT"
);

test_deserializer!(
    test_priority_arabic_normal,
    r#"عادي"#,
    priority_enum(),
    empty_db(),
    "NORMAL"
);

test_failing_deserializer!(
    test_original_priority_values_fail_with_aliases,
    r#"HIGH"#,
    priority_enum(),
    empty_db()
);

// original values are not allowed in lists
// so we should return an empty list
#[test]
fn test_multiple_original_enum_values_fail() {
    let priority = priority_enum();
    let mut db = TypeRefDb::new();
    db.try_add("Priority", priority).ok().unwrap();
    let target_ty = array_of(annotated(Ty::Unresolved("Priority")));

    let raw = r#"["HIGH", "MEDIUM", "LOW"]"#;
    let parsed =
        crate::jsonish::parse(raw, Default::default(), true).expect("jsonish::parse failed");
    let ctx = crate::deserializer::coercer::ParsingContext::new(target_ty.as_ref(), &db);
    let annots = TypeAnnotations::default();
    let target = TyWithMeta::new(target_ty.as_ref(), &annots);
    let result = TyResolvedRef::coerce(&ctx, target, &parsed);
    assert!(result.is_ok(), "Failed to parse: {:?}", result);
    let value = result.unwrap().unwrap();
    let json_value = serde_json::to_value(&value).unwrap();
    // since "médium" -> "MEDIUM"
    let expected = serde_json::json!(["MEDIUM"]);
    assert_eq!(json_value, expected);
}

// =============================================================================
// LITERAL TESTS WITH ACCENTED VALUES
// =============================================================================

test_deserializer!(
    test_literal_string_french_name,
    r#"François"#,
    literal_string("François"),
    empty_db(),
    "François"
);

test_deserializer!(
    test_literal_string_spanish_greeting,
    r#"¡Hola!"#,
    literal_string("¡Hola!"),
    empty_db(),
    "¡Hola!"
);

test_deserializer!(
    test_literal_string_portuguese_word,
    r#"São Paulo"#,
    literal_string("São Paulo"),
    empty_db(),
    "São Paulo"
);

test_deserializer!(
    test_literal_string_german_umlaut,
    r#"Müller"#,
    literal_string("Müller"),
    empty_db(),
    "Müller"
);

test_deserializer!(
    test_literal_string_chinese_characters,
    r#"北京"#,
    literal_string("北京"),
    empty_db(),
    "北京"
);

test_deserializer!(
    test_literal_string_arabic_text,
    r#"السلام عليكم"#,
    literal_string("السلام عليكم"),
    empty_db(),
    "السلام عليكم"
);

test_deserializer!(
    test_literal_string_russian_cyrillic,
    r#"Москва"#,
    literal_string("Москва"),
    empty_db(),
    "Москва"
);

test_deserializer!(
    test_literal_string_japanese_hiragana,
    r#"こんにちは"#,
    literal_string("こんにちは"),
    empty_db(),
    "こんにちは"
);

test_deserializer!(
    test_literal_string_accented_with_quotes,
    r#""François""#,
    literal_string("François"),
    empty_db(),
    "François"
);

test_deserializer!(
    test_literal_string_accented_case_insensitive,
    r#"françois"#,
    literal_string("François"),
    empty_db(),
    "François"
);

test_deserializer!(
    test_literal_string_accented_in_sentence,
    r#"The name is François for this person"#,
    literal_string("François"),
    empty_db(),
    "François"
);

test_deserializer!(
    test_literal_string_cafe_with_emoji,
    r#"Café ☕"#,
    literal_string("Café ☕"),
    empty_db(),
    "Café ☕"
);

test_deserializer!(
    test_union_literal_city_names,
    r#"São Paulo"#,
    union_of(vec![
        annotated(literal_string("Paris")),
        annotated(literal_string("São Paulo")),
        annotated(literal_string("Zürich")),
    ]),
    empty_db(),
    "São Paulo"
);

test_deserializer!(
    test_union_literal_mixed_languages,
    r#"北京"#,
    union_of(vec![
        annotated(literal_string("Paris")),
        annotated(literal_string("北京")),
        annotated(literal_string("القاهرة")),
    ]),
    empty_db(),
    "北京"
);

test_deserializer!(
    test_literal_string_diacritics_combination,
    r#"naïve résumé"#,
    literal_string("naïve résumé"),
    empty_db(),
    "naïve résumé"
);

// =============================================================================
// CLASS TESTS WITH ACCENTED ALIASES
// =============================================================================

fn restaurant_class() -> TyResolved<'static, &'static str> {
    class_ty(
        "Restaurant",
        vec![
            field_with_aliases("name", string_ty(), vec!["nom"]),
            field_with_aliases("address", string_ty(), vec!["adresse"]),
            field_with_aliases("specialty", string_ty(), vec!["spécialité"]),
            field_with_aliases("stars", int_ty(), vec!["étoiles"]),
        ],
    )
}

test_deserializer!(
  test_french_field_aliases,
  r#"{"nom": "Le Petit Café", "adresse": "Champs-Élysées", "spécialité": "crêpes bretonnes", "étoiles": 4}"#,
  restaurant_class(),
  empty_db(),
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
  restaurant_class(),
  empty_db(),
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
    restaurant_class(),
    empty_db()
);

fn international_contact_class() -> TyResolved<'static, &'static str> {
    class_ty(
        "InternationalContact",
        vec![
            field_with_aliases("first_name", string_ty(), vec!["prénom"]),
            field_with_aliases("family_name", string_ty(), vec!["família"]),
            field_with_aliases("city", string_ty(), vec!["città"]),
            field_with_aliases("street", string_ty(), vec!["straße"]),
            field_with_aliases("field", string_ty(), vec!["поле"]),
            field_with_aliases("data_field", string_ty(), vec!["フィールド"]),
        ],
    )
}

test_deserializer!(
  test_international_field_aliases,
  r#"{"prénom": "François", "família": "Silva", "città": "Milano", "straße": "Hauptstraße", "поле": "значение", "フィールド": "値"}"#,
  international_contact_class(),
  empty_db(),
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
  international_contact_class(),
  empty_db(),
  {
    "first_name": "José",
    "family_name": "González",
    "city": "Barcelona",
    "street": "Königstraße",
    "field": "текст",
    "data_field": "データ"
  }
);

#[test]
fn test_french_nested_class_aliases() {
    let address = class_ty(
        "Address",
        vec![
            field_with_aliases("number", int_ty(), vec!["numéro"]),
            field_with_aliases("street", string_ty(), vec!["rue"]),
            field_with_aliases("city", string_ty(), vec!["ville"]),
            field_with_aliases("region", string_ty(), vec!["région"]),
        ],
    );
    let mut db = TypeRefDb::new();
    db.try_add("Address", address).ok().unwrap();
    let person = class_ty(
        "Person",
        vec![
            field_with_aliases("first_name", string_ty(), vec!["prénom"]),
            field_with_aliases("last_name", string_ty(), vec!["nom"]),
            field_with_aliases("age", int_ty(), vec!["âge"]),
            field_with_aliases("address", Ty::Unresolved("Address"), vec!["adresse"]),
        ],
    );

    let raw = r#"{"prénom": "François", "nom": "Müller", "âge": 35, "adresse": {"numéro": 42, "rue": "Champs-Élysées", "ville": "Paris", "région": "Île-de-France"}}"#;
    let parsed =
        crate::jsonish::parse(raw, Default::default(), true).expect("jsonish::parse failed");
    let ctx = crate::deserializer::coercer::ParsingContext::new(person.as_ref(), &db);
    let annots = TypeAnnotations::default();
    let target = TyWithMeta::new(person.as_ref(), &annots);
    let result = TyResolvedRef::coerce(&ctx, target, &parsed);
    assert!(result.is_ok(), "Failed to parse: {:?}", result);
    let value = result.unwrap().unwrap();
    let json_value = serde_json::to_value(&value).unwrap();
    let expected = serde_json::json!({
        "first_name": "François",
        "last_name": "Müller",
        "age": 35,
        "address": {
            "number": 42,
            "street": "Champs-Élysées",
            "city": "Paris",
            "region": "Île-de-France"
        }
    });
    assert_eq!(json_value, expected);
}

fn product_info_class() -> TyResolved<'static, &'static str> {
    class_ty(
        "ProductInfo",
        vec![
            field_with_aliases("name", string_ty(), vec!["nom"]),
            field_with_aliases("price", float_ty(), vec!["prix"]),
            field_with_aliases("category", string_ty(), vec!["catégorie"]),
            field_with_aliases("description", string_ty(), vec!["description"]),
        ],
    )
}

test_deserializer!(
  test_class_with_accented_aliases,
  r#"{"nom": "Café Latte", "prix": 4.50, "catégorie": "Boissons", "description": "Délicieux café"}"#,
  product_info_class(),
  empty_db(),
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
    product_info_class(),
    empty_db()
);

test_deserializer!(
  test_library_with_french_aliases,
  r#"{"livres": ["L'Étranger", "Amélie Poulain", "Naïveté"], "auteurs": ["Camus", "Jeunet", "Müller"], "années": [1942, 2001, 2020]}"#,
  class_ty("Library", vec![
      field_with_aliases("books", array_of(annotated(string_ty())), vec!["livres"]),
      field_with_aliases("authors", array_of(annotated(string_ty())), vec!["auteurs"]),
      field_with_aliases("years", array_of(annotated(int_ty())), vec!["années"]),
  ]),
  empty_db(),
  {
    "books": ["L'Étranger", "Amélie Poulain", "Naïveté"],
    "authors": ["Camus", "Jeunet", "Müller"],
    "years": [1942, 2001, 2020]
  }
);

// =============================================================================
// UNACCENTED INPUT MATCHING ACCENTED ALIASES
// =============================================================================

fn spanish_title_enum() -> TyResolved<'static, &'static str> {
    enum_ty(
        "SpanishTitle",
        vec![
            variant_with_aliases("MISTER", vec!["señor"]),
            variant_with_aliases("MISS", vec!["señorita"]),
            variant_with_aliases("DOCTOR", vec!["doctor"]),
            variant_with_aliases("PROFESSOR", vec!["profesor"]),
        ],
    )
}

test_deserializer!(
    test_unaccented_senor_matches_accented_alias,
    r#"senor"#,
    spanish_title_enum(),
    empty_db(),
    "MISTER"
);

test_deserializer!(
    test_unaccented_senorita_matches_accented_alias,
    r#"senorita"#,
    spanish_title_enum(),
    empty_db(),
    "MISS"
);

test_deserializer!(
    test_unaccented_profesor_matches_accented_alias,
    r#"profesor"#,
    spanish_title_enum(),
    empty_db(),
    "PROFESSOR"
);

test_deserializer!(
    test_unaccented_in_sentence,
    r#"The title is senor for this person"#,
    spanish_title_enum(),
    empty_db(),
    "MISTER"
);

fn french_words_enum() -> TyResolved<'static, &'static str> {
    enum_ty(
        "FrenchWords",
        vec![
            variant_with_aliases("COFFEE", vec!["café"]),
            variant_with_aliases("NAIVE", vec!["naïve"]),
            variant_with_aliases("RESUME", vec!["résumé"]),
            variant_with_aliases("ELITE", vec!["élite"]),
            variant_with_aliases("FACADE", vec!["façade"]),
        ],
    )
}

test_deserializer!(
    test_unaccented_cafe_matches_accented_alias,
    r#"cafe"#,
    french_words_enum(),
    empty_db(),
    "COFFEE"
);

test_deserializer!(
    test_unaccented_naive_matches_accented_alias,
    r#"naive"#,
    french_words_enum(),
    empty_db(),
    "NAIVE"
);

test_deserializer!(
    test_unaccented_resume_matches_accented_alias,
    r#"resume"#,
    french_words_enum(),
    empty_db(),
    "RESUME"
);

test_deserializer!(
    test_unaccented_elite_matches_accented_alias,
    r#"elite"#,
    french_words_enum(),
    empty_db(),
    "ELITE"
);

test_deserializer!(
    test_unaccented_facade_matches_accented_alias,
    r#"facade"#,
    french_words_enum(),
    empty_db(),
    "FACADE"
);

#[test]
fn test_unaccented_french_in_list() {
    let fw = french_words_enum();
    let mut db = TypeRefDb::new();
    db.try_add("FrenchWords", fw).ok().unwrap();
    let target_ty = array_of(annotated(Ty::Unresolved("FrenchWords")));

    let raw = r#"["cafe", "naive", "resume"]"#;
    let parsed =
        crate::jsonish::parse(raw, Default::default(), true).expect("jsonish::parse failed");
    let ctx = crate::deserializer::coercer::ParsingContext::new(target_ty.as_ref(), &db);
    let annots = TypeAnnotations::default();
    let target = TyWithMeta::new(target_ty.as_ref(), &annots);
    let result = TyResolvedRef::coerce(&ctx, target, &parsed);
    assert!(result.is_ok(), "Failed to parse: {:?}", result);
    let value = result.unwrap().unwrap();
    let json_value = serde_json::to_value(&value).unwrap();
    let expected = serde_json::json!(["COFFEE", "NAIVE", "RESUME"]);
    assert_eq!(json_value, expected);
}

fn german_words_enum() -> TyResolved<'static, &'static str> {
    enum_ty(
        "GermanWords",
        vec![
            variant_with_aliases("OVER", vec!["über"]),
            variant_with_aliases("LEADER", vec!["führer"]),
            variant_with_aliases("DOOR", vec!["tür"]),
            variant_with_aliases("GREEN", vec!["grün"]),
        ],
    )
}

test_deserializer!(
    test_unaccented_uber_matches_accented_alias,
    r#"uber"#,
    german_words_enum(),
    empty_db(),
    "OVER"
);

test_deserializer!(
    test_unaccented_fuhrer_matches_accented_alias,
    r#"fuhrer"#,
    german_words_enum(),
    empty_db(),
    "LEADER"
);

test_deserializer!(
    test_unaccented_tur_matches_accented_alias,
    r#"tur"#,
    german_words_enum(),
    empty_db(),
    "DOOR"
);

test_deserializer!(
    test_unaccented_grun_matches_accented_alias,
    r#"grun"#,
    german_words_enum(),
    empty_db(),
    "GREEN"
);

// CLASS TESTS WITH UNACCENTED INPUT MATCHING ACCENTED ALIASES

fn spanish_form_class() -> TyResolved<'static, &'static str> {
    class_ty(
        "SpanishForm",
        vec![
            field_with_aliases("title", string_ty(), vec!["señor"]),
            field_with_aliases("name", string_ty(), vec!["nombre"]),
            field_with_aliases("age", int_ty(), vec!["edad"]),
            field_with_aliases("address", string_ty(), vec!["dirección"]),
        ],
    )
}

test_deserializer!(
  test_unaccented_class_field_senor,
  r#"{"senor": "Sr. García", "nombre": "Juan", "edad": 30, "direccion": "Calle Mayor 123"}"#,
  spanish_form_class(),
  empty_db(),
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
  spanish_form_class(),
  empty_db(),
  {
    "title": "Sr. García",
    "name": "Juan",
    "age": 30,
    "address": "Calle Mayor 123"
  }
);

fn french_profile_class() -> TyResolved<'static, &'static str> {
    class_ty(
        "FrenchProfile",
        vec![
            field_with_aliases("first_name", string_ty(), vec!["prénom"]),
            field_with_aliases("last_name", string_ty(), vec!["nom"]),
            field_with_aliases("city", string_ty(), vec!["ville"]),
            field_with_aliases("profession", string_ty(), vec!["métier"]),
        ],
    )
}

test_deserializer!(
  test_unaccented_french_class_fields,
  r#"{"prenom": "François", "nom": "Dupont", "ville": "Paris", "metier": "Professeur"}"#,
  french_profile_class(),
  empty_db(),
  {
    "first_name": "François",
    "last_name": "Dupont",
    "city": "Paris",
    "profession": "Professeur"
  }
);

fn portuguese_data_class() -> TyResolved<'static, &'static str> {
    class_ty(
        "PortugueseData",
        vec![
            field_with_aliases("location", string_ty(), vec!["localização"]),
            field_with_aliases("description", string_ty(), vec!["descrição"]),
            field_with_aliases("solution", string_ty(), vec!["solução"]),
            field_with_aliases("information", string_ty(), vec!["informação"]),
        ],
    )
}

test_deserializer!(
  test_unaccented_portuguese_class_fields,
  r#"{"localizacao": "São Paulo", "descricao": "Uma cidade grande", "solucao": "Transporte público", "informacao": "Dados importantes"}"#,
  portuguese_data_class(),
  empty_db(),
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
    literal_string("café"),
    empty_db(),
    "café"
);

test_deserializer!(
    test_unaccented_literal_resume,
    r#"resume"#,
    literal_string("résumé"),
    empty_db(),
    "résumé"
);

test_deserializer!(
    test_unaccented_literal_senor,
    r#"senor"#,
    literal_string("señor"),
    empty_db(),
    "señor"
);

test_deserializer!(
    test_unaccented_literal_in_union,
    r#"cafe"#,
    union_of(vec![
        annotated(literal_string("café")),
        annotated(literal_string("résumé")),
        annotated(literal_string("señor")),
    ]),
    empty_db(),
    "café"
);

// =============================================================================
// CASE-INSENSITIVE UNACCENTED MATCHING TESTS
// =============================================================================

fn spanish_greeting_enum() -> TyResolved<'static, &'static str> {
    enum_ty(
        "SpanishGreeting",
        vec![
            variant_with_aliases("HELLO", vec!["Hola"]),
            variant_with_aliases("GOODBYE", vec!["Adiós"]),
            variant_with_aliases("PLEASE", vec!["Por favor"]),
            variant_with_aliases("THANK_YOU", vec!["Gracias"]),
        ],
    )
}

test_deserializer!(
    test_case_insensitive_unaccented_hola,
    r#"hola"#,
    spanish_greeting_enum(),
    empty_db(),
    "HELLO"
);

test_deserializer!(
    test_case_insensitive_unaccented_adios,
    r#"adios"#,
    spanish_greeting_enum(),
    empty_db(),
    "GOODBYE"
);

test_deserializer!(
    test_case_insensitive_mixed_case_unaccented,
    r#"ADIOS"#,
    spanish_greeting_enum(),
    empty_db(),
    "GOODBYE"
);

test_deserializer!(
    test_case_insensitive_unaccented_por_favor,
    r#"por favor"#,
    spanish_greeting_enum(),
    empty_db(),
    "PLEASE"
);

fn french_food_enum() -> TyResolved<'static, &'static str> {
    enum_ty(
        "FrenchFood",
        vec![
            variant_with_aliases("COFFEE", vec!["Café"]),
            variant_with_aliases("CAKE", vec!["Gâteau"]),
            variant_with_aliases("CHEESE", vec!["Fromage"]),
            variant_with_aliases("BREAD", vec!["Pain"]),
        ],
    )
}

test_deserializer!(
    test_case_insensitive_unaccented_cafe_upper,
    r#"CAFE"#,
    french_food_enum(),
    empty_db(),
    "COFFEE"
);

test_deserializer!(
    test_case_insensitive_unaccented_gateau,
    r#"gateau"#,
    french_food_enum(),
    empty_db(),
    "CAKE"
);

test_deserializer!(
    test_case_insensitive_mixed_case_fromage,
    r#"FrOmAgE"#,
    french_food_enum(),
    empty_db(),
    "CHEESE"
);

test_deserializer!(
    test_case_insensitive_french_in_sentence,
    r#"I would like some CAFE please"#,
    french_food_enum(),
    empty_db(),
    "COFFEE"
);

// Test case insensitive unaccented matching in class fields

fn german_address_class() -> TyResolved<'static, &'static str> {
    class_ty(
        "GermanAddress",
        vec![
            field_with_aliases("street", string_ty(), vec!["Straße"]),
            field_with_aliases("city", string_ty(), vec!["Stadt"]),
            field_with_aliases("over", string_ty(), vec!["Über"]),
            field_with_aliases("green", string_ty(), vec!["Grün"]),
        ],
    )
}

test_deserializer!(
  test_case_insensitive_unaccented_german_fields,
  r#"{"strasse": "Main St", "stadt": "Berlin", "uber": "Above", "grun": "Green"}"#,
  german_address_class(),
  empty_db(),
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
  german_address_class(),
  empty_db(),
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
    literal_string("señor"),
    empty_db(),
    "señor"
);

test_deserializer!(
    test_case_insensitive_unaccented_literal_resume_mixed,
    r#"ReSuMe"#,
    literal_string("résumé"),
    empty_db(),
    "résumé"
);

test_deserializer!(
    test_case_insensitive_unaccented_literal_naive_lower,
    r#"naive"#,
    literal_string("Naïve"),
    empty_db(),
    "Naïve"
);

// Test combinations of case variations and accents
fn complex_accents_enum() -> TyResolved<'static, &'static str> {
    enum_ty(
        "ComplexAccents",
        vec![
            variant_with_aliases("WORD1", vec!["Señorita"]),
            variant_with_aliases("WORD2", vec!["CAFÉ"]),
            variant_with_aliases("WORD3", vec!["résumé"]),
            variant_with_aliases("WORD4", vec!["NAÏVE"]),
            variant_with_aliases("WORD5", vec!["Über"]),
        ],
    )
}

test_deserializer!(
    test_complex_case_unaccented_senorita_lower,
    r#"senorita"#,
    complex_accents_enum(),
    empty_db(),
    "WORD1"
);

test_deserializer!(
    test_complex_case_unaccented_cafe_lower,
    r#"cafe"#,
    complex_accents_enum(),
    empty_db(),
    "WORD2"
);

test_deserializer!(
    test_complex_case_unaccented_resume_upper,
    r#"RESUME"#,
    complex_accents_enum(),
    empty_db(),
    "WORD3"
);

test_deserializer!(
    test_complex_case_unaccented_naive_mixed,
    r#"NaIvE"#,
    complex_accents_enum(),
    empty_db(),
    "WORD4"
);

test_deserializer!(
    test_complex_case_unaccented_uber_lower,
    r#"uber"#,
    complex_accents_enum(),
    empty_db(),
    "WORD5"
);

#[test]
fn test_complex_case_unaccented_list_mixed_cases() {
    let ce = complex_accents_enum();
    let mut db = TypeRefDb::new();
    db.try_add("ComplexAccents", ce).ok().unwrap();
    let target_ty = array_of(annotated(Ty::Unresolved("ComplexAccents")));

    let raw = r#"["SENORITA", "cafe", "Resume", "naive", "UBER"]"#;
    let parsed =
        crate::jsonish::parse(raw, Default::default(), true).expect("jsonish::parse failed");
    let ctx = crate::deserializer::coercer::ParsingContext::new(target_ty.as_ref(), &db);
    let annots = TypeAnnotations::default();
    let target = TyWithMeta::new(target_ty.as_ref(), &annots);
    let result = TyResolvedRef::coerce(&ctx, target, &parsed);
    assert!(result.is_ok(), "Failed to parse: {:?}", result);
    let value = result.unwrap().unwrap();
    let json_value = serde_json::to_value(&value).unwrap();
    let expected = serde_json::json!(["WORD1", "WORD2", "WORD3", "WORD4", "WORD5"]);
    assert_eq!(json_value, expected);
}

// Test case insensitive unaccented with punctuation
fn punctuation_accents_enum() -> TyResolved<'static, &'static str> {
    enum_ty(
        "PunctuationAccents",
        vec![
            variant_with_aliases("TEST1", vec!["señor-josé"]),
            variant_with_aliases("TEST2", vec!["café_bar"]),
            variant_with_aliases("TEST3", vec!["résumé.doc"]),
            variant_with_aliases("TEST4", vec!["naïve-approach"]),
        ],
    )
}

test_deserializer!(
    test_case_insensitive_unaccented_with_punctuation,
    r#"SENOR-JOSE"#,
    punctuation_accents_enum(),
    empty_db(),
    "TEST1"
);

test_deserializer!(
    test_case_insensitive_unaccented_cafe_bar,
    r#"cafe_bar"#,
    punctuation_accents_enum(),
    empty_db(),
    "TEST2"
);

test_deserializer!(
    test_case_insensitive_unaccented_resume_doc,
    r#"resume doc"#,
    punctuation_accents_enum(),
    empty_db(),
    "TEST3"
);
