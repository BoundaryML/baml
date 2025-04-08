class DynamicClass {
    static_prop string
    @@dynamic
}

function ReturnDynamicClass(input: string) -> DynamicClass {
  client "openai/gpt-4o"
    prompt #"
        Extract data from INPUT.
        {{ ctx.output_format }}

        INPUT:
        ---
        {{ input }}
        ---
    "#
}

// Assume for this request we
// are extracting a Resume. So now we can add
// all properties to extract at runtime (test-time!)
test DynamicClassTest {
    functions [ReturnDynamicClass]
    type_builder {
        dynamic class DynamicClass {
            name string
            email string
            experience Experience[]
            skills string[]
        }

        class Experience {
          position string
          company string
        }
    }
    args {
      input #"
        Vaibhav Gupta
        vbv@boundaryml.com

        Experience:
        - Founder at BoundaryML
        - CV Engineer at Google
        - CV Engineer at Microsoft

        Skills:
        - Rust
        - C++
    "#
    }
}
