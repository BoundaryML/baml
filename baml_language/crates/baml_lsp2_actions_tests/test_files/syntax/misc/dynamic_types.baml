class Resume {
  name string
  education Education[]
  skills string[]
  @@dynamic // This class is marked as @@dynamic.
}

class Education {
  school string
  degree string
  year int
}

enum Job {
  SoftwareEngineer
  DataScientist
  @@dynamic
}

// This function returns the dynamic class defined above.
function ExtractResume(from_text: string) -> Resume {
  client "openai/gpt-4o-mini"
  prompt #"Hello"#
}

test ReturnDynamicClassTest {
  functions [ExtractResume]
  // New type_builder block used to define types and inject dynamic props.
  type_builder {
    // Defines a new type available only within this test block.
    class Experience {
      title string
      company string
      start_date string
      end_date string
    }

    enum Level {
      Junior
      Mid
      Senior
    }

    // This `dynamic` block is used to inject new properties into the
    // `@@dynamic` part of the Resume class.
    dynamic class Resume {
      experience Experience[]
      level Level
    }

    dynamic enum Job {
      ProductManager
    }
  }
  args {
    from_text #"
      John Doe

      Education
      - University of California, Berkeley, B.S. in Computer Science, 2020

      Experience
      - Software Engineer, Boundary, Sep 2022 - Sep 2023

      Skills
      - Python
      - Java
    "#
  }
}

//----
//- diagnostics
// E0098
//
//   × removed language feature
//     ╭─[misc_dynamic_types.baml:29:3]
//  29 │   type_builder {
//     ·   ──────┬─────
//     ·         ╰── `type_builder` blocks were removed; runtime type construction is `baml.reflect` (BEP-066)
//     ╰────
// E0098
//
//   × `@@dynamic` was removed; runtime type construction is `baml.reflect` (BEP-066)
//    ╭─[misc_dynamic_types.baml:5:3]
//  5 │   @@dynamic // This class is marked as @@dynamic.
//    ·   ────┬────
//    ·       ╰── removed attribute
//    ╰────
// E0098
//
//   × `@@dynamic` was removed; runtime type construction is `baml.reflect` (BEP-066)
//     ╭─[misc_dynamic_types.baml:17:3]
//  17 │   @@dynamic
//     ·   ────┬────
//     ·       ╰── removed attribute
//     ╰────
