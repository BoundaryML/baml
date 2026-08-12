use pyo3_stub_gen::Result;

fn main() -> Result<()> {
    let stub = baml_py::stub_info()?;
    stub.generate()?;
    Ok(())
}
