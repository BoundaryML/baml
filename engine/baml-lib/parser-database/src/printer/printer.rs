use std::collections::{HashMap, HashSet};

use pyo3::{
    types::{PyDict, PyFunction},
    PyAny, PyErr, PyResult, Python,
};
use regex::Regex;

pub(crate) struct Printer<'a> {
    printer_fn: &'a PyFunction,
    json_dumps: &'a PyFunction,
}

impl Printer<'_> {
    pub fn create<'a>(
        py: Python<'a>,
        required_funcs: &[&str],
        default_code: &str,
        printer_code: Option<&str>,
    ) -> Result<Printer<'a>, String> {
        let globals = PyDict::new(py);
        match run_python_code(py, default_code, &globals) {
            Ok(_) => {}
            Err(e) => {
                return Err(format!(
                    "Failed to setup defaults. Unrecoverable: {}",
                    format_python_exception(py, &e)
                ));
            }
        }

        if let Some(code) = printer_code {
            match run_python_code(py, code, &globals) {
                Ok(_) => {}
                Err(e) => {
                    return Err(format!(
                        "Failed to set up printer code. {}",
                        format_python_exception(py, &e)
                    ));
                }
            }
        }

        // Validate that all required functions are present.
        let mut missing = Vec::new();
        let mut first_func = None;
        for &func_name in required_funcs {
            let func = globals.get_item(func_name);
            if func.is_err() {
                missing.push(func_name);
            } else {
                let func = func.unwrap();
                match func.map(|f| f.downcast::<pyo3::types::PyFunction>()) {
                    Some(Ok(a)) => {
                        if &func_name == required_funcs.first().unwrap() {
                            first_func = Some(a);
                        }
                    }
                    _ => missing.push(func_name),
                }
            }
        }

        if !missing.is_empty() {
            return Err(format!("Missing definition for: {}", missing.join(", ")));
        }

        let sys = py.import("json").map_err(|e| e.to_string())?;

        let json_dumps = sys
            .getattr("loads")
            .map_err(|e| e.to_string())?
            .downcast::<pyo3::types::PyFunction>()
            .map_err(|e| e.to_string())?;

        Ok(Printer {
            printer_fn: first_func.unwrap(),
            json_dumps,
        })
    }

    pub fn print(&self, py: Python<'_>, data: serde_json::Value) -> Result<String, String> {
        let outcome = self
            .json_dumps
            .call1((data.to_string(),))
            .map_err(|e| e.to_string())?;
        let py_result: Result<&PyAny, _> = self.printer_fn.call1((outcome,));

        match py_result {
            Ok(py_string) => {
                // Convert the PyAny to a Rust string.
                py_string
                    .downcast::<pyo3::types::PyString>()
                    .map(|s| s.to_string())
                    .map_err(|e| e.to_string())
            }
            Err(err) => Err(format!(
                "Error excuting serailizer: {}",
                format_python_exception(py, &err)
            )),
        }
    }
}

fn format_python_exception(py: Python<'_>, err: &PyErr) -> String {
    // Attempt to obtain the PyBaseException instance

    err.print(py);

    err.to_string()
}

fn run_python_code(py: Python<'_>, code: &str, globals: &PyDict) -> PyResult<()> {
    // Extract any import / from X import Y lines vs the actual code into two
    // separate lists.
    let mut imports = Vec::new();
    let code = code
        .lines()
        .filter(|line| {
            if line.starts_with("import ") || line.starts_with("from ") {
                imports.push(line.to_string());
                false
            } else {
                true
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    if !imports.is_empty() {
        // Determine every import that is required.
        let (modules, import_items) = extract_python_imports(&imports.join("\n"));
        for module in modules.iter() {
            let _mod = py.import(module.0.as_str())?;
            if let Some(_alias) = &module.1 {
                globals.set_item(_alias.as_str(), _mod)?;
            } else {
                globals.set_item(module.0.as_str(), _mod)?;
            }
        }

        for (module, items) in import_items.iter() {
            let py_module = py.import(module.as_str())?;
            for (item, alias) in items.iter() {
                let py_item = py_module.getattr(item.as_str())?;
                let name_to_insert = alias.as_ref().unwrap_or_else(|| item);
                globals.set_item(name_to_insert, py_item)?;
            }
        }
    }

    if !code.is_empty() {
        run_python_code_impl(py, &code, &globals)?;
    }

    Ok(())
}

fn run_python_code_impl(py: Python<'_>, code: &str, globals: &PyDict) -> PyResult<()> {
    let locals = PyDict::new(py);

    py.run(&code, Some(&globals), Some(locals))?;
    // Merge all the locals into the globals.
    for (key, value) in locals {
        globals.set_item(key, value)?;
    }

    Ok(())
}

fn extract_python_imports(
    code: &str,
) -> (
    HashSet<(String, Option<String>)>,
    HashMap<String, Vec<(String, Option<String>)>>,
) {
    let import_regex = Regex::new(r"^import (\w+(\.\w+)*(?: as \w+)?)").unwrap();
    let from_import_regex =
        Regex::new(r"^from (\w+(\.\w+)*) import (\w+(?: as \w+)?(?:, \w+(?: as \w+)?)*)").unwrap();

    let mut modules = HashSet::new();
    let mut imports = HashMap::new();

    for capture in import_regex.captures_iter(code) {
        if let Some(module) = capture.get(1) {
            let parts: Vec<&str> = module.as_str().split(" as ").collect();
            let module_name = parts[0].to_string();
            let as_specifier = if parts.len() > 1 {
                Some(parts[1].to_string())
            } else {
                None
            };

            modules.insert((module_name.clone(), as_specifier));
        }
    }

    for capture in from_import_regex.captures_iter(code) {
        if let Some(module) = capture.get(1) {
            let items = capture.get(3).unwrap().as_str();
            for item in items.split(", ") {
                let parts: Vec<&str> = item.split(" as ").collect();
                let import_name = parts[0].trim().to_string();
                let as_specifier = if parts.len() > 1 {
                    Some(parts[1].trim().to_string())
                } else {
                    None
                };

                imports
                    .entry(module.as_str().to_string())
                    .or_insert_with(Vec::new)
                    .push((import_name, as_specifier));
            }
        }
    }

    (modules, imports)
}
