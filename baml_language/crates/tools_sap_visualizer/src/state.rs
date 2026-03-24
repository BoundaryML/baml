//! Simulates SAP streaming. Given a JSON-like input and SAP model type,
//! will try all the partial deserializations and emit a visualization of the deserialized
//! values as the input is streamed in.

use ::bex_sap::{
    deserializer::coercer::{ParsingContext, ParsingError},
    jsonish,
    sap_model::{self, TyResolvedRef},
};
use ::eframe::egui;
use ::std::rc::Rc;

/// Cached result for a single prefix parse.
type CachedResult = Option<Result<Option<String>, ParsingError>>;

pub struct SapVisualizerState<N: sap_model::TypeIdent + 'static> {
    json: Rc<String>,
    ty: Rc<SapVisualizerTypes<'static, N>>,
    output: Vec<CachedResult>,
}
impl<N: sap_model::TypeIdent + 'static> SapVisualizerState<N> {
    pub fn new(
        json: String,
        db: sap_model::TypeRefDb<'static, N>,
        ty: sap_model::AnnotatedTy<'static, N>,
    ) -> Self {
        let ty = Rc::new(SapVisualizerTypes { db, ty });
        Self::new_impl(json, ty)
    }

    fn new_impl(json: String, ty: Rc<SapVisualizerTypes<'static, N>>) -> Self {
        let json = Rc::new(json);
        if json.is_empty() {
            return Self {
                json,
                ty,
                output: vec![None],
            };
        }

        let mut output = Vec::new();

        for (end, _) in json
            .char_indices()
            .chain(std::iter::once((json.len(), '\0')))
        {
            output.push(Self::parse_item(&json, &ty, end));
        }
        Self { json, ty, output }
    }

    fn parse_item(json: &str, ty: &SapVisualizerTypes<'static, N>, end: usize) -> CachedResult {
        assert!(end <= json.len());

        let slice = &json[..end];
        let Ok(jsonish) = jsonish::parse(slice, Default::default(), end + 1 == json.len()) else {
            return None;
        };

        let ctx = ParsingContext::new(&ty.db);
        let sap = ty
            .db
            .resolve_with_meta(ty.ty.as_ref())
            .map_err(|n| ParsingError {
                scope: ctx.scope.clone(),
                reason: format!("Failed to resolve top-level type: {n}"),
                causes: Vec::new(),
            })
            .and_then(|value| TyResolvedRef::coerce(&ctx, value, &jsonish));

        Some(match sap {
            Ok(sap) => Ok(Some(serde_json::to_string(&sap).unwrap())),
            Err(e) => Err(e),
        })
    }

    /// More efficient if only the json changes.
    ///
    /// Since most of the time the json change is at the end (add or remove suffix),
    /// we check to see if we can keep some of the existing output.
    pub fn with_json(mut self, json: String) -> Self {
        self.update_with_json(json);
        self
    }

    pub fn json(&self) -> &str {
        &self.json
    }

    pub fn db(&self) -> &bex_sap::sap_model::TypeRefDb<'static, N> {
        &self.ty.db
    }

    pub fn ty(&self) -> &bex_sap::sap_model::AnnotatedTy<'static, N> {
        &self.ty.ty
    }

    /// More efficient if only the json changes.
    ///
    /// Since most of the time the json change is at the end (add or remove suffix),
    /// we check to see if we can keep some of the existing output.
    pub fn update_with_json(&mut self, json: String) {
        let mut keep_len_bytes = 0;
        let mut keep_len_chars = 0;
        let mut it_old = self.json.chars();
        let mut it_new = json.chars();
        loop {
            match (it_old.next(), it_new.next()) {
                (None, None) => return, // entirely the same
                (Some(old), Some(new)) if old == new => {
                    keep_len_chars += 1;
                    keep_len_bytes += old.len_utf8();
                }
                _ => break,
            };
        }

        assert!(keep_len_chars <= self.output.len());
        self.output.truncate(keep_len_chars);
        self.json = Rc::new(json);
        let suffix = &self.json[keep_len_bytes..];
        for (end_offset, _) in suffix
            .char_indices()
            .chain(std::iter::once((suffix.len(), '\0')))
        {
            let end = keep_len_bytes + end_offset;
            self.output
                .push(Self::parse_item(&self.json, &self.ty, end));
        }

        debug_assert_eq!(self.json.chars().count() + 1, self.output.len());
    }

    pub fn iter(&self) -> impl Iterator<Item = &CachedResult> {
        self.output.iter()
    }
}

struct SapVisualizerTypes<'t, N: sap_model::TypeIdent + 't> {
    db: sap_model::TypeRefDb<'t, N>,
    ty: sap_model::AnnotatedTy<'t, N>,
}

impl<N: sap_model::TypeIdent + 'static> egui::TextBuffer for SapVisualizerState<N> {
    fn is_mutable(&self) -> bool {
        true
    }

    fn as_str(&self) -> &str {
        &self.json
    }

    fn insert_text(&mut self, text: &str, char_index: usize) -> usize {
        let mut json = self.json.to_string();
        let inserted = json.insert_text(text, char_index);
        self.update_with_json(json);
        inserted
    }

    fn delete_char_range(&mut self, char_range: std::ops::Range<usize>) {
        let mut json = self.json.to_string();
        json.delete_char_range(char_range);
        self.update_with_json(json);
    }

    fn type_id(&self) -> std::any::TypeId {
        std::any::TypeId::of::<Self>()
    }
}
