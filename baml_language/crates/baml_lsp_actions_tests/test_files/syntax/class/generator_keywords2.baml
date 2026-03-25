// This file is just like generator_keywords1.baml, except that the fieldname
// has been changed in order to not collide with the field type `ETA`, and an
// alias is used to render that field as `ETA` in prompts.

generator lang_python {
  output_type "python/pydantic"
  output_dir "../python"
  version "0.68.0"
}

class ETA {
    thing string
}

class Foo {
    eta ETA? @alias("ETA")
}

//----
//- diagnostics
// <no-diagnostics-expected>
