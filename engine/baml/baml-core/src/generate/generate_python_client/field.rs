// use internal_baml_parser_database::walkers::Walker;
// use internal_baml_schema_ast::ast::{ClassId, FieldArity, FieldId, FieldType, TypeValue};

// use super::{file::FileCollector, traits::WithToCode};

// impl WithUpdatePythonString for Walker<'_, (ClassId, FieldId)> {
//     fn update_py_file<'a>(&'a self, fc: &'a mut super::file::File) {
//         fc.add_indent_string(format!("{}: ", self.name()), 1);
//         self.r#type().update_py_file(fc);

//         if self.r#type().0 == FieldArity::Optional {
//             fc.add_string(" = None");
//         }
//         fc.add_empty_line();
//     }
// }

// impl WithUpdatePythonString for (FieldArity, &FieldType) {
//     fn update_py_file<'a>(&'a self, fc: &'a mut super::file::File) {
//         match self.0 {
//             FieldArity::Required => match self.1 {
//                 FieldType::PrimitiveType(s, _) => s.update_py_file(fc),
//                 FieldType::Supported(idn) => fc.add_string(&idn.name),
//                 FieldType::Union(types, _) => {
//                     fc.add_import("typing", "Union");
//                     fc.add_string("Union[");
//                     for (i, (arity, field)) in types.iter().enumerate() {
//                         if i > 0 {
//                             fc.add_string(", ");
//                         }
//                         (arity.clone(), field).update_py_file(fc);
//                     }
//                     fc.add_string("]");
//                 }
//                 FieldType::Unsupported(..) => panic!("Unsupported field type"),
//             },
//             FieldArity::List => {
//                 fc.add_import("typing", "List");
//                 fc.add_string("List[");
//                 (FieldArity::Required, self.1).update_py_file(fc);
//                 fc.add_string("]");
//             }
//             FieldArity::Optional => {
//                 fc.add_import("typing", "Optional");
//                 fc.add_string("Optional[");
//                 (FieldArity::Required, self.1).update_py_file(fc);
//                 fc.add_string("]");
//             }
//         }
//     }
// }

// impl WithUpdatePythonString for TypeValue {
//     fn update_py_file<'a>(&'a self, fc: &'a mut super::file::File) {
//         match self {
//             TypeValue::String => fc.add_string("str"),
//             TypeValue::Int => fc.add_string("int"),
//             TypeValue::Float => fc.add_string("float"),
//             TypeValue::Boolean => fc.add_string("bool"),
//             TypeValue::Char => fc.add_string("str"),
//         }
//     }
// }
