
//# Main Function
function ComplexHeaderTest(x: int) -> int {
    //# increment by 10
    if (x < 10) {
        x = x + 10;
    } else {
        //## dummy variable y
        let y = 5;
    }
    
    //# john deere
    let f = ExtractResume("john deere makes tractors");
    
    //# john deere 2
    let f2 = ExtractResume("john deere makes tractors");
    
    //# another increment
    x + 1234
}


class Resume {
  name string
  education Education[] @description("Extract in the same order listed")
  skills string[] @description("Only include programming languages")
}

class Education {
  school string
  degree string
  year int
}

function ExtractResume(resume_text: string) -> Resume {
  // see clients.baml
  client "openai/gpt-4o"

  // The prompt uses Jinja syntax. Change the models or this text and watch the prompt preview change!
  prompt #"
    Parse the following resume and return a structured representation of the data in the schema below.

    Resume:
    ---
    {{ resume_text }}
    ---

    {# special macro to print the output instructions. #}
    {{ ctx.output_format }}

    JSON:
  "#
}
test Foo {
    functions [ComplexHeaderTest]
    args {
        x 5
    }
}
