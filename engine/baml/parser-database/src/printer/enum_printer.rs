use pyo3::Python;

use super::printer::Printer;

pub(crate) fn setup_printer<'a>(
    py: Python<'a>,
    printer_code: Option<&str>,
) -> Result<Printer<'a>, String> {
    let default_code = include_str!("./templates/print_enum_default.py");
    Printer::create(
        py,
        &["print_enum", "print_enum_value"],
        default_code,
        printer_code,
    )
}
