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
// Error: Expected '}', found type_builder
//     ╭─[ misc_dynamic_types.baml:29:3 ]
//     │
//  29 │   type_builder {
//     │   ──────┬─────  
//     │         ╰─────── Expected '}', found type_builder
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found type_builder
//     ╭─[ misc_dynamic_types.baml:29:3 ]
//     │
//  29 │   type_builder {
//     │   ──────┬─────  
//     │         ╰─────── Expected top-level declaration, found type_builder
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '{'
//     ╭─[ misc_dynamic_types.baml:29:16 ]
//     │
//  29 │   type_builder {
//     │                ┬  
//     │                ╰── Expected top-level declaration, found '{'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found dynamic
//     ╭─[ misc_dynamic_types.baml:46:5 ]
//     │
//  46 │     dynamic class Resume {
//     │     ───┬───  
//     │        ╰───── Expected top-level declaration, found dynamic
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found dynamic
//     ╭─[ misc_dynamic_types.baml:51:5 ]
//     │
//  51 │     dynamic enum Job {
//     │     ───┬───  
//     │        ╰───── Expected top-level declaration, found dynamic
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '}'
//     ╭─[ misc_dynamic_types.baml:54:3 ]
//     │
//  54 │   }
//     │   ┬  
//     │   ╰── Expected top-level declaration, found '}'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found identifier
//     ╭─[ misc_dynamic_types.baml:55:3 ]
//     │
//  55 │   args {
//     │   ──┬─  
//     │     ╰─── Expected top-level declaration, found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '{'
//     ╭─[ misc_dynamic_types.baml:55:8 ]
//     │
//  55 │   args {
//     │        ┬  
//     │        ╰── Expected top-level declaration, found '{'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found identifier
//     ╭─[ misc_dynamic_types.baml:56:5 ]
//     │
//  56 │     from_text #"
//     │     ────┬────  
//     │         ╰────── Expected top-level declaration, found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '#'
//     ╭─[ misc_dynamic_types.baml:56:15 ]
//     │
//  56 │     from_text #"
//     │               ┬  
//     │               ╰── Expected top-level declaration, found '#'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '"'
//     ╭─[ misc_dynamic_types.baml:56:16 ]
//     │
//  56 │     from_text #"
//     │                ┬  
//     │                ╰── Expected top-level declaration, found '"'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found identifier
//     ╭─[ misc_dynamic_types.baml:57:7 ]
//     │
//  57 │       John Doe
//     │       ──┬─  
//     │         ╰─── Expected top-level declaration, found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found identifier
//     ╭─[ misc_dynamic_types.baml:57:12 ]
//     │
//  57 │       John Doe
//     │            ─┬─  
//     │             ╰─── Expected top-level declaration, found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found identifier
//     ╭─[ misc_dynamic_types.baml:59:7 ]
//     │
//  59 │       Education
//     │       ────┬────  
//     │           ╰────── Expected top-level declaration, found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '-'
//     ╭─[ misc_dynamic_types.baml:60:7 ]
//     │
//  60 │       - University of California, Berkeley, B.S. in Computer Science, 2020
//     │       ┬  
//     │       ╰── Expected top-level declaration, found '-'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found identifier
//     ╭─[ misc_dynamic_types.baml:60:9 ]
//     │
//  60 │       - University of California, Berkeley, B.S. in Computer Science, 2020
//     │         ─────┬────  
//     │              ╰────── Expected top-level declaration, found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found identifier
//     ╭─[ misc_dynamic_types.baml:60:20 ]
//     │
//  60 │       - University of California, Berkeley, B.S. in Computer Science, 2020
//     │                    ─┬  
//     │                     ╰── Expected top-level declaration, found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found identifier
//     ╭─[ misc_dynamic_types.baml:60:23 ]
//     │
//  60 │       - University of California, Berkeley, B.S. in Computer Science, 2020
//     │                       ─────┬────  
//     │                            ╰────── Expected top-level declaration, found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found ','
//     ╭─[ misc_dynamic_types.baml:60:33 ]
//     │
//  60 │       - University of California, Berkeley, B.S. in Computer Science, 2020
//     │                                 ┬  
//     │                                 ╰── Expected top-level declaration, found ','
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found identifier
//     ╭─[ misc_dynamic_types.baml:60:35 ]
//     │
//  60 │       - University of California, Berkeley, B.S. in Computer Science, 2020
//     │                                   ────┬───  
//     │                                       ╰───── Expected top-level declaration, found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found ','
//     ╭─[ misc_dynamic_types.baml:60:43 ]
//     │
//  60 │       - University of California, Berkeley, B.S. in Computer Science, 2020
//     │                                           ┬  
//     │                                           ╰── Expected top-level declaration, found ','
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found identifier
//     ╭─[ misc_dynamic_types.baml:60:45 ]
//     │
//  60 │       - University of California, Berkeley, B.S. in Computer Science, 2020
//     │                                             ┬  
//     │                                             ╰── Expected top-level declaration, found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '.'
//     ╭─[ misc_dynamic_types.baml:60:46 ]
//     │
//  60 │       - University of California, Berkeley, B.S. in Computer Science, 2020
//     │                                              ┬  
//     │                                              ╰── Expected top-level declaration, found '.'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found identifier
//     ╭─[ misc_dynamic_types.baml:60:47 ]
//     │
//  60 │       - University of California, Berkeley, B.S. in Computer Science, 2020
//     │                                               ┬  
//     │                                               ╰── Expected top-level declaration, found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '.'
//     ╭─[ misc_dynamic_types.baml:60:48 ]
//     │
//  60 │       - University of California, Berkeley, B.S. in Computer Science, 2020
//     │                                                ┬  
//     │                                                ╰── Expected top-level declaration, found '.'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found in
//     ╭─[ misc_dynamic_types.baml:60:50 ]
//     │
//  60 │       - University of California, Berkeley, B.S. in Computer Science, 2020
//     │                                                  ─┬  
//     │                                                   ╰── Expected top-level declaration, found in
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found identifier
//     ╭─[ misc_dynamic_types.baml:60:53 ]
//     │
//  60 │       - University of California, Berkeley, B.S. in Computer Science, 2020
//     │                                                     ────┬───  
//     │                                                         ╰───── Expected top-level declaration, found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found identifier
//     ╭─[ misc_dynamic_types.baml:60:62 ]
//     │
//  60 │       - University of California, Berkeley, B.S. in Computer Science, 2020
//     │                                                              ───┬───  
//     │                                                                 ╰───── Expected top-level declaration, found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found ','
//     ╭─[ misc_dynamic_types.baml:60:69 ]
//     │
//  60 │       - University of California, Berkeley, B.S. in Computer Science, 2020
//     │                                                                     ┬  
//     │                                                                     ╰── Expected top-level declaration, found ','
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found integer
//     ╭─[ misc_dynamic_types.baml:60:71 ]
//     │
//  60 │       - University of California, Berkeley, B.S. in Computer Science, 2020
//     │                                                                       ──┬─  
//     │                                                                         ╰─── Expected top-level declaration, found integer
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found identifier
//     ╭─[ misc_dynamic_types.baml:62:7 ]
//     │
//  62 │       Experience
//     │       ─────┬────  
//     │            ╰────── Expected top-level declaration, found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '-'
//     ╭─[ misc_dynamic_types.baml:63:7 ]
//     │
//  63 │       - Software Engineer, Boundary, Sep 2022 - Sep 2023
//     │       ┬  
//     │       ╰── Expected top-level declaration, found '-'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found identifier
//     ╭─[ misc_dynamic_types.baml:63:9 ]
//     │
//  63 │       - Software Engineer, Boundary, Sep 2022 - Sep 2023
//     │         ────┬───  
//     │             ╰───── Expected top-level declaration, found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found identifier
//     ╭─[ misc_dynamic_types.baml:63:18 ]
//     │
//  63 │       - Software Engineer, Boundary, Sep 2022 - Sep 2023
//     │                  ────┬───  
//     │                      ╰───── Expected top-level declaration, found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found ','
//     ╭─[ misc_dynamic_types.baml:63:26 ]
//     │
//  63 │       - Software Engineer, Boundary, Sep 2022 - Sep 2023
//     │                          ┬  
//     │                          ╰── Expected top-level declaration, found ','
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found identifier
//     ╭─[ misc_dynamic_types.baml:63:28 ]
//     │
//  63 │       - Software Engineer, Boundary, Sep 2022 - Sep 2023
//     │                            ────┬───  
//     │                                ╰───── Expected top-level declaration, found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found ','
//     ╭─[ misc_dynamic_types.baml:63:36 ]
//     │
//  63 │       - Software Engineer, Boundary, Sep 2022 - Sep 2023
//     │                                    ┬  
//     │                                    ╰── Expected top-level declaration, found ','
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found identifier
//     ╭─[ misc_dynamic_types.baml:63:38 ]
//     │
//  63 │       - Software Engineer, Boundary, Sep 2022 - Sep 2023
//     │                                      ─┬─  
//     │                                       ╰─── Expected top-level declaration, found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found integer
//     ╭─[ misc_dynamic_types.baml:63:42 ]
//     │
//  63 │       - Software Engineer, Boundary, Sep 2022 - Sep 2023
//     │                                          ──┬─  
//     │                                            ╰─── Expected top-level declaration, found integer
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '-'
//     ╭─[ misc_dynamic_types.baml:63:47 ]
//     │
//  63 │       - Software Engineer, Boundary, Sep 2022 - Sep 2023
//     │                                               ┬  
//     │                                               ╰── Expected top-level declaration, found '-'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found identifier
//     ╭─[ misc_dynamic_types.baml:63:49 ]
//     │
//  63 │       - Software Engineer, Boundary, Sep 2022 - Sep 2023
//     │                                                 ─┬─  
//     │                                                  ╰─── Expected top-level declaration, found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found integer
//     ╭─[ misc_dynamic_types.baml:63:53 ]
//     │
//  63 │       - Software Engineer, Boundary, Sep 2022 - Sep 2023
//     │                                                     ──┬─  
//     │                                                       ╰─── Expected top-level declaration, found integer
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found identifier
//     ╭─[ misc_dynamic_types.baml:65:7 ]
//     │
//  65 │       Skills
//     │       ───┬──  
//     │          ╰──── Expected top-level declaration, found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '-'
//     ╭─[ misc_dynamic_types.baml:66:7 ]
//     │
//  66 │       - Python
//     │       ┬  
//     │       ╰── Expected top-level declaration, found '-'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found identifier
//     ╭─[ misc_dynamic_types.baml:66:9 ]
//     │
//  66 │       - Python
//     │         ───┬──  
//     │            ╰──── Expected top-level declaration, found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '-'
//     ╭─[ misc_dynamic_types.baml:67:7 ]
//     │
//  67 │       - Java
//     │       ┬  
//     │       ╰── Expected top-level declaration, found '-'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found identifier
//     ╭─[ misc_dynamic_types.baml:67:9 ]
//     │
//  67 │       - Java
//     │         ──┬─  
//     │           ╰─── Expected top-level declaration, found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '"'
//     ╭─[ misc_dynamic_types.baml:68:5 ]
//     │
//  68 │     "#
//     │     ┬  
//     │     ╰── Expected top-level declaration, found '"'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '#'
//     ╭─[ misc_dynamic_types.baml:68:6 ]
//     │
//  68 │     "#
//     │      ┬  
//     │      ╰── Expected top-level declaration, found '#'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '}'
//     ╭─[ misc_dynamic_types.baml:69:3 ]
//     │
//  69 │   }
//     │   ┬  
//     │   ╰── Expected top-level declaration, found '}'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '}'
//     ╭─[ misc_dynamic_types.baml:70:1 ]
//     │
//  70 │ }
//     │ ┬  
//     │ ╰── Expected top-level declaration, found '}'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Duplicate class 'Resume'
//    ╭─[ misc_dynamic_types.baml:1:1 ]
//    │
//  1 │ class Resume {
//    │ │ 
//    │ ╰─ class 'Resume' defined in misc_dynamic_types.baml
//    │ │ 
//    │ ╰─ 'Resume' previously defined in misc_dynamic_types.baml
//    │ 
//    │ Note: Error code: E0011
// ───╯
// Error: Duplicate enum 'Job'
//    ╭─[ misc_dynamic_types.baml:1:1 ]
//    │
//  1 │ class Resume {
//    │ │ 
//    │ ╰─ enum 'Job' defined in misc_dynamic_types.baml
//    │ │ 
//    │ ╰─ 'Job' previously defined in misc_dynamic_types.baml
//    │ 
//    │ Note: Error code: E0011
// ───╯
