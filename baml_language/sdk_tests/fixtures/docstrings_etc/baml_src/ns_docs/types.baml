// Doc-comment coverage. Each `///`-annotated declaration below pins a
// specific Python emission rule:
//
//   * /// on class           -> """...""" body docstring on the class
//   * /// on field           -> folded into class docstring under
//                                "Attributes:" (no inline `# …` line)
//   * /// on enum            -> """...""" body docstring on the enum
//   * /// on enum variant    -> folded into enum docstring under
//                                "Members:" (no inline PEP-257 form)
//
// Attributes:/Members: section visibility follows an "any-doc" rule:
//   * if no field/variant carries a ///, the section is suppressed
//     entirely — even when the class/enum itself has a ///
//   * if at least one carries a ///, the section appears and lists
//     **every** member: documented as `name: doc`, undocumented as
//     a bare `name` (no trailing colon)

/// A document with a title and an optional body.
class Doc {
  /// Title shown in lists and search results.
  title string
  /// Free-form body text.
  body string?
}

/// A multi-line summary.
/// Continuation line of the summary, preserved verbatim in the
/// rendered block-form docstring.
class Note {
  /// Stable identifier — surfaces in URLs.
  id string
  // `text` carries no `///`. With the "any-doc" rule above, it still
  // appears under Attributes: as a bare `text` because `id` is
  // documented.
  text string
}

/// Sentiment labels surfaced by the model.
enum Sentiment {
  /// Smiling face.
  HAPPY
  /// Frowning face.
  SAD
  // `NEUTRAL` is undocumented; it appears under Members: as a bare
  // `NEUTRAL` because the enum has at least one documented variant.
  NEUTRAL
}

/// Pin the "summary only, no member rollup" case: this enum has a
/// class-level `///` but every variant is bare.
enum Priority {
  HIGH
  MEDIUM
  LOW
}
