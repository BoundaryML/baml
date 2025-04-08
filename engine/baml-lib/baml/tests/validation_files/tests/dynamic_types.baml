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
