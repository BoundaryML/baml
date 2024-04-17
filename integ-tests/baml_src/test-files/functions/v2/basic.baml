

function ExtractResume2(resume: string) -> Resume {
    client Resilient_ComplexSyntax
    prompt #"
        {{ _.chat('system') }}

        Extract the following information from the resume:

        Resume:
        <<<<
        {{ resume }}
        <<<<

        Output JSON schema:
        {{ ctx.output_schema }}

        JSON:
    "#
}


class WithReasoning {
    value string
    reasoning string @description(#"
        Why the value is a good fit.
    "#)
}


class SearchParams {
    dateRange int? @description(#"
        In ISO duration format, e.g. P1Y2M10D.
    "#)
    location string[]
    jobTitle WithReasoning? @description(#"
        An exact job title, not a general category.
    "#)
    company WithReasoning? @description(#"
        The exact name of the company, not a product or service.
    "#)
    description WithReasoning[] @description(#"
        Any specific projects or features the user is looking for.
    "#)
    tags (Tag | string)[]
}

enum Tag {
    Security
    AI
    Blockchain
}

function GetQuery(query: string) -> SearchParams {
    client GPT4
    prompt #"
        Extract the following information from the query:

        Query:
        <<<<
        {{ query }}
        <<<<

        OUTPUT_JSON_SCHEMA:
        {{ ctx.output_schema }}

        Before OUTPUT_JSON_SCHEMA, list 5 intentions the user may have.
        --- EXAMPLES ---
        1. <intent>
        2. <intent>
        3. <intent>
        4. <intent>
        5. <intent>

        {
            ... // OUTPUT_JSON_SCHEMA
        }
    "#
}

class RaysData {
    dataType DataType
    value Resume | Event
}

enum DataType {
    Resume
    Event
}

class Event {
    title string
    date string
    location string
    description string
}

function GetDataType(text: string) -> RaysData {
    client GPT4
    prompt #"
        Extract the relevant info.

        Text:
        <<<<
        {{ text }}
        <<<<

        Output JSON schema:
        {{ ctx.output_schema }}

        JSON:
    "#
}