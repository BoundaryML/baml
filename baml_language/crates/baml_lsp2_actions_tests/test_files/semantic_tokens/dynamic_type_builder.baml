class Resume {
    name: string,
    education: Education[],
    skills: string[],
    @@dynamic // This class is marked as @@dynamic.
}

class Education {
    school: string,
    degree: string,
    year: int,
}

enum Job {
    SoftwareEngineer,
    DataScientist,
    @@dynamic
}

// This function returns the dynamic class defined above.
function ExtractResume(from_text: string) -> Resume {
    client: "openai/gpt-4o-mini"
    prompt: `Hello ${from_text} ${ctx.output_format}`
}

test ReturnDynamicClassTest {
    functions: [ExtractResume],
    // New type_builder block used to define types and inject dynamic props.
    type_builder {
        // Defines a new type available only within this test block.
        class Experience {
            title: string,
            company: string,
            start_date: string,
            end_date: string,
        }

        enum Level {
            Junior,
            Mid,
            Senior,
        }

        // This `dynamic` block is used to inject new properties into the
        // `@@dynamic` part of the Resume class.
        dynamic class Resume {
            experience: Experience[],
            level: Level,
        }

        dynamic enum Job {
            ProductManager,
        }
    },
    args: {
        from_text: `
            John Doe

            Education
            - University of California, Berkeley, B.S. in Computer Science, 2020

            Experience
            - Software Engineer, Boundary, Sep 2022 - Sep 2023

            Skills
            - Python
            - Java
        `,
    },
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// dynamic_type_builder.baml:1:1 (keyword) len=5 "class"
// dynamic_type_builder.baml:1:7 (class) [declaration] len=6 "Resume"
// dynamic_type_builder.baml:2:5 (property) [declaration] len=4 "name"
// dynamic_type_builder.baml:2:11 (type) [defaultLibrary] len=6 "string"
// dynamic_type_builder.baml:3:5 (property) [declaration] len=9 "education"
// dynamic_type_builder.baml:3:16 (class) len=9 "Education"
// dynamic_type_builder.baml:4:5 (property) [declaration] len=6 "skills"
// dynamic_type_builder.baml:4:13 (type) [defaultLibrary] len=6 "string"
// dynamic_type_builder.baml:5:5 (decorator) len=2 "@@"
// dynamic_type_builder.baml:5:7 (decorator) len=7 "dynamic"
// dynamic_type_builder.baml:5:15 (comment) len=37 "// This class is marked as @@dynamic."
// dynamic_type_builder.baml:8:1 (keyword) len=5 "class"
// dynamic_type_builder.baml:8:7 (class) [declaration] len=9 "Education"
// dynamic_type_builder.baml:9:5 (property) [declaration] len=6 "school"
// dynamic_type_builder.baml:9:13 (type) [defaultLibrary] len=6 "string"
// dynamic_type_builder.baml:10:5 (property) [declaration] len=6 "degree"
// dynamic_type_builder.baml:10:13 (type) [defaultLibrary] len=6 "string"
// dynamic_type_builder.baml:11:5 (property) [declaration] len=4 "year"
// dynamic_type_builder.baml:11:11 (type) [defaultLibrary] len=3 "int"
// dynamic_type_builder.baml:14:1 (keyword) len=4 "enum"
// dynamic_type_builder.baml:14:6 (enum) [declaration] len=3 "Job"
// dynamic_type_builder.baml:15:5 (enumMember) [declaration] len=16 "SoftwareEngineer"
// dynamic_type_builder.baml:16:5 (enumMember) [declaration] len=13 "DataScientist"
// dynamic_type_builder.baml:17:5 (decorator) len=2 "@@"
// dynamic_type_builder.baml:17:7 (decorator) len=7 "dynamic"
// dynamic_type_builder.baml:20:1 (comment) len=57 "// This function returns the dynamic class defined above."
// dynamic_type_builder.baml:21:1 (keyword) len=8 "function"
// dynamic_type_builder.baml:21:10 (function) [declaration] len=13 "ExtractResume"
// dynamic_type_builder.baml:21:24 (parameter) [declaration] len=9 "from_text"
// dynamic_type_builder.baml:21:35 (type) [defaultLibrary] len=6 "string"
// dynamic_type_builder.baml:21:46 (class) len=6 "Resume"
// dynamic_type_builder.baml:22:5 (property) len=6 "client"
// dynamic_type_builder.baml:22:13 (string) len=20 "\"openai/gpt-4o-mini\""
// dynamic_type_builder.baml:23:5 (property) [declaration] len=6 "prompt"
// dynamic_type_builder.baml:23:13 (string) len=1 "`"
// dynamic_type_builder.baml:23:14 (string) len=5 "Hello"
// dynamic_type_builder.baml:23:22 (parameter) len=9 "from_text"
// dynamic_type_builder.baml:23:39 (property) len=13 "output_format"
// dynamic_type_builder.baml:23:53 (string) len=1 "`"
// dynamic_type_builder.baml:26:1 (keyword) len=4 "test"
// dynamic_type_builder.baml:26:6 (struct) [declaration] len=22 "ReturnDynamicClassTest"
// dynamic_type_builder.baml:27:5 (property) len=9 "functions"
// dynamic_type_builder.baml:28:5 (comment) len=72 "// New type_builder block used to define types and inject dynamic props."
// dynamic_type_builder.baml:29:5 (keyword) len=12 "type_builder"
// dynamic_type_builder.baml:30:9 (comment) len=60 "// Defines a new type available only within this test block."
// dynamic_type_builder.baml:31:9 (keyword) len=5 "class"
// dynamic_type_builder.baml:31:15 (class) [declaration] len=10 "Experience"
// dynamic_type_builder.baml:32:13 (property) [declaration] len=5 "title"
// dynamic_type_builder.baml:32:20 (type) [defaultLibrary] len=6 "string"
// dynamic_type_builder.baml:33:13 (property) [declaration] len=7 "company"
// dynamic_type_builder.baml:33:22 (type) [defaultLibrary] len=6 "string"
// dynamic_type_builder.baml:34:13 (property) [declaration] len=10 "start_date"
// dynamic_type_builder.baml:34:25 (type) [defaultLibrary] len=6 "string"
// dynamic_type_builder.baml:35:13 (property) [declaration] len=8 "end_date"
// dynamic_type_builder.baml:35:23 (type) [defaultLibrary] len=6 "string"
// dynamic_type_builder.baml:38:9 (keyword) len=4 "enum"
// dynamic_type_builder.baml:38:14 (enum) [declaration] len=5 "Level"
// dynamic_type_builder.baml:39:13 (enumMember) [declaration] len=6 "Junior"
// dynamic_type_builder.baml:40:13 (enumMember) [declaration] len=3 "Mid"
// dynamic_type_builder.baml:41:13 (enumMember) [declaration] len=6 "Senior"
// dynamic_type_builder.baml:44:9 (comment) len=65 "// This `dynamic` block is used to inject new properties into the"
// dynamic_type_builder.baml:45:9 (comment) len=40 "// `@@dynamic` part of the Resume class."
// dynamic_type_builder.baml:46:9 (keyword) len=7 "dynamic"
// dynamic_type_builder.baml:46:17 (keyword) len=5 "class"
// dynamic_type_builder.baml:46:23 (class) [declaration] len=6 "Resume"
// dynamic_type_builder.baml:47:13 (property) [declaration] len=10 "experience"
// dynamic_type_builder.baml:47:25 (type) len=10 "Experience"
// dynamic_type_builder.baml:48:13 (property) [declaration] len=5 "level"
// dynamic_type_builder.baml:48:20 (type) len=5 "Level"
// dynamic_type_builder.baml:51:9 (keyword) len=7 "dynamic"
// dynamic_type_builder.baml:51:17 (keyword) len=4 "enum"
// dynamic_type_builder.baml:51:22 (enum) [declaration] len=3 "Job"
// dynamic_type_builder.baml:52:13 (enumMember) [declaration] len=14 "ProductManager"
// dynamic_type_builder.baml:55:5 (property) len=4 "args"
// dynamic_type_builder.baml:56:9 (property) len=9 "from_text"
// dynamic_type_builder.baml:56:20 (string) len=1 "`"
// dynamic_type_builder.baml:57:13 (string) len=4 "John"
// dynamic_type_builder.baml:57:18 (string) len=3 "Doe"
// dynamic_type_builder.baml:59:13 (string) len=9 "Education"
// dynamic_type_builder.baml:60:13 (string) len=1 "-"
// dynamic_type_builder.baml:60:15 (string) len=10 "University"
// dynamic_type_builder.baml:60:26 (string) len=2 "of"
// dynamic_type_builder.baml:60:29 (string) len=10 "California"
// dynamic_type_builder.baml:60:39 (string) len=1 ","
// dynamic_type_builder.baml:60:41 (string) len=8 "Berkeley"
// dynamic_type_builder.baml:60:49 (string) len=1 ","
// dynamic_type_builder.baml:60:51 (string) len=1 "B"
// dynamic_type_builder.baml:60:52 (string) len=1 "."
// dynamic_type_builder.baml:60:53 (string) len=1 "S"
// dynamic_type_builder.baml:60:54 (string) len=1 "."
// dynamic_type_builder.baml:60:56 (string) len=2 "in"
// dynamic_type_builder.baml:60:59 (string) len=8 "Computer"
// dynamic_type_builder.baml:60:68 (string) len=7 "Science"
// dynamic_type_builder.baml:60:75 (string) len=1 ","
// dynamic_type_builder.baml:60:77 (string) len=4 "2020"
// dynamic_type_builder.baml:62:13 (string) len=10 "Experience"
// dynamic_type_builder.baml:63:13 (string) len=1 "-"
// dynamic_type_builder.baml:63:15 (string) len=8 "Software"
// dynamic_type_builder.baml:63:24 (string) len=8 "Engineer"
// dynamic_type_builder.baml:63:32 (string) len=1 ","
// dynamic_type_builder.baml:63:34 (string) len=8 "Boundary"
// dynamic_type_builder.baml:63:42 (string) len=1 ","
// dynamic_type_builder.baml:63:44 (string) len=3 "Sep"
// dynamic_type_builder.baml:63:48 (string) len=4 "2022"
// dynamic_type_builder.baml:63:53 (string) len=1 "-"
// dynamic_type_builder.baml:63:55 (string) len=3 "Sep"
// dynamic_type_builder.baml:63:59 (string) len=4 "2023"
// dynamic_type_builder.baml:65:13 (string) len=6 "Skills"
// dynamic_type_builder.baml:66:13 (string) len=1 "-"
// dynamic_type_builder.baml:66:15 (string) len=6 "Python"
// dynamic_type_builder.baml:67:13 (string) len=1 "-"
// dynamic_type_builder.baml:67:15 (string) len=4 "Java"
// dynamic_type_builder.baml:68:9 (string) len=1 "`"
