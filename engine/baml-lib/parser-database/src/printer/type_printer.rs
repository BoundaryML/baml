use pyo3::Python;

use super::printer::Printer;

pub(crate) fn setup_printer<'a>(
    py: Python<'a>,
    printer_code: Option<&str>,
) -> Result<Printer<'a>, String> {
    let default_code = include_str!("./templates/print_type_default.py");

    let required_funcs = [
        "print_type",
        "print_optional",
        "print_primitive",
        "print_class",
        "print_list",
        "print_enum",
        "print_union",
    ];
    Printer::create(py, &required_funcs, default_code, printer_code)
}
