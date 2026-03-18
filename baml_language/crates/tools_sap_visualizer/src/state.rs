//! Simulates SAP streaming. Given a JSON-like input and SAP model type,
//! will try all the partial deserializations and emit a visualization of the deserialized
//! values as the input is streamed in.

use ::bex_sap::{
    deserializer::{
        coercer::{ParsingContext, ParsingError},
        types::BamlValueWithFlags,
    },
    jsonish,
    sap_model::{self, TyResolvedRef},
};
use ::eframe::egui;
use ::ouroboros::self_referencing;
use ::std::rc::Rc;

pub struct SapVisualizerState<N: sap_model::TypeIdent + 'static> {
    json: Rc<String>,
    ty: Rc<SapVisualizerTypes<'static, N>>,
    output: Vec<Option<SapVisualizerOutput<N>>>,
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
            let json = json.clone();
            let ty = ty.clone();
            output.push(Self::parse_item(json, ty, end));
        }
        Self { json, ty, output }
    }

    fn parse_item(
        json: Rc<String>,
        ty: Rc<SapVisualizerTypes<'static, N>>,
        end: usize,
    ) -> Option<SapVisualizerOutput<N>> {
        assert!(end <= json.len());

        let Ok(jsonish) = SapVisualizerOutputJsonish::try_new(json.clone(), |json| {
            let slice = &json[..end];
            jsonish::parse(slice, Default::default(), end + 1 == json.len())
        }) else {
            return None;
        };

        let item = SapVisualizerOutput::new(jsonish, ty, |jsonish, ty| {
            let ctx = ParsingContext::new(&ty.db);
            let value = ty
                .db
                .resolve_with_meta(ty.ty.as_ref())
                .map_err(|n| ParsingError {
                    scope: ctx.scope.clone(),
                    reason: format!("Failed to resolve top-level type: {n}"),
                    causes: Vec::new(),
                })?;
            TyResolvedRef::coerce(&ctx, value, jsonish.borrow_jsonish())
        });

        Some(item)
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
        if self.json.starts_with(&json) {
            // New json is the old json with characters removed from the end.
            self.json = Rc::new(json);
            while self.output.len() > self.json.len() + 1 {
                self.output.pop();
            }
        } else if json.starts_with(self.json.as_str()) {
            // New json is the old json with characters added to the end.
            let old_len = self.json.len();
            self.json = Rc::new(json);
            for end in old_len + 1..=self.json.len() {
                self.output
                    .push(Self::parse_item(self.json.clone(), self.ty.clone(), end));
            }
        } else {
            // was somewhere in the middle
            *self = Self::new_impl(json, self.ty.clone());
        }
        debug_assert_eq!(self.json.chars().count() + 1, self.output.len());
    }

    pub fn iter(&self) -> impl Iterator<Item = Option<Result<Option<String>, ParsingError>>> {
        self.output.iter().map(|o| {
            o.as_ref().map(|v| {
                v.with_sap(|sap| match sap {
                    Ok(sap) => Ok(Some(serde_json::to_string(&sap).unwrap())),
                    Err(e) => Err(e.clone()),
                })
            })
        })
    }
}

struct SapVisualizerTypes<'t, N: sap_model::TypeIdent + 't> {
    db: sap_model::TypeRefDb<'t, N>,
    ty: sap_model::AnnotatedTy<'t, N>,
}

#[self_referencing]
struct SapVisualizerOutputJsonish {
    json: Rc<String>,
    #[borrows(json)]
    #[covariant]
    jsonish: jsonish::Value<'this>,
}

#[self_referencing]
struct SapVisualizerOutput<N: sap_model::TypeIdent + 'static> {
    jsonish: SapVisualizerOutputJsonish,
    ty: Rc<SapVisualizerTypes<'static, N>>,
    #[borrows(jsonish, ty)]
    #[not_covariant]
    sap: Result<Option<BamlValueWithFlags<'this, 'this, 'this, N>>, ParsingError>,
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
