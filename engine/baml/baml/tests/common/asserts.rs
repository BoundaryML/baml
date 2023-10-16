use baml::internal_baml_diagnostics::DatamodelWarning;
use baml::internal_baml_parser_database::ScalarType;
use baml::internal_baml_schema_ast::ast::{self};
use baml::StringFromEnvVar;

pub(crate) trait DatamodelAssert<'a> {
    // fn assert_has_model(&'a self, name: &str) -> walkers::ModelWalker<'a>;
    // fn assert_has_type(&'a self, name: &str) -> walkers::CompositeTypeWalker<'a>;
}

pub(crate) trait DatasourceAsserts {
    fn assert_name(&self, name: &str) -> &Self;
    fn assert_url(&self, url: StringFromEnvVar) -> &Self;
}

pub(crate) trait WarningAsserts {
    fn assert_is(&self, warning: DatamodelWarning) -> &Self;
}

pub(crate) trait TypeAssert<'a> {
    // fn assert_has_scalar_field(&self, t: &str) -> walkers::CompositeTypeFieldWalker<'a>;
}

pub(crate) trait ScalarFieldAssert {
    fn assert_scalar_type(&self, t: ScalarType) -> &Self;
    // fn assert_is_single_field_id(&self) -> walkers::PrimaryKeyWalker<'_>;
    // fn assert_is_single_field_unique(&self) -> walkers::IndexWalker<'_>;
    fn assert_not_single_field_unique(&self) -> &Self;
    fn assert_ignored(&self, ignored: bool) -> &Self;
    fn assert_with_documentation(&self, t: &str) -> &Self;
    fn assert_required(&self) -> &Self;
    fn assert_optional(&self) -> &Self;
    fn assert_list(&self) -> &Self;
    // fn assert_default_value(&self) -> walkers::DefaultValueWalker<'_>;
    fn assert_mapped_name(&self, name: &str) -> &Self;
    fn assert_is_updated_at(&self) -> &Self;
}

pub(crate) trait DefaultValueAssert {
    fn assert_string(&self, val: &str) -> &Self;
    fn assert_int(&self, val: usize) -> &Self;
    fn assert_float(&self, val: f32) -> &Self;
    fn assert_bool(&self, val: bool) -> &Self;
    fn assert_constant(&self, val: &str) -> &Self;
    fn assert_bytes(&self, val: &[u8]) -> &Self;
}

impl WarningAsserts for Vec<DatamodelWarning> {
    #[track_caller]
    fn assert_is(&self, warning: DatamodelWarning) -> &Self {
        assert_eq!(
            self.len(),
            1,
            "Expected exactly one validation warning. Warnings are: {:?}",
            &self
        );
        assert_eq!(self[0], warning);
        self
    }
}

impl DefaultValueAssert for ast::Expression {
    #[track_caller]
    fn assert_string(&self, expected: &str) -> &Self {
        match self {
            ast::Expression::StringValue(actual, _) => assert_eq!(actual, expected),
            _ => panic!("Not a string value"),
        }

        self
    }

    #[track_caller]
    fn assert_int(&self, expected: usize) -> &Self {
        match self {
            ast::Expression::NumericValue(actual, _) => assert_eq!(actual, &format!("{expected}")),
            _ => panic!("Not a number value"),
        }

        self
    }

    #[track_caller]
    fn assert_float(&self, expected: f32) -> &Self {
        match self {
            ast::Expression::NumericValue(actual, _) => assert_eq!(actual, &format!("{expected}")),
            _ => panic!("Not a number value"),
        }

        self
    }

    #[track_caller]
    fn assert_bool(&self, expected: bool) -> &Self {
        assert!(
            matches!(self, ast::Expression::ConstantValue(actual, _) if actual == &format!("{expected}"))
        );

        self
    }

    #[track_caller]
    fn assert_constant(&self, expected: &str) -> &Self {
        assert!(matches!(self, ast::Expression::ConstantValue(actual, _) if actual == expected));

        self
    }

    #[track_caller]
    fn assert_bytes(&self, expected: &[u8]) -> &Self {
        match self {
            ast::Expression::StringValue(actual, _) => {
                assert_eq!(base64::decode(actual).unwrap(), expected)
            }
            _ => panic!("Not a bytes value"),
        }

        self
    }
}
