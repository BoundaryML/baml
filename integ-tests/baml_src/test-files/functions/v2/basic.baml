

function ExtractResume2(resume: string) -> Resume {
    client Ollama
    prompt #"
        {{ _.role('system') }}

        Extract the following information from the resume:

        Resume:
        <<<<
        {{ resume }}
        <<<<

        Output JSON schema:
        {{ ctx.output_format }}

        JSON:
    "#
}
test er2 {
    functions[ExtractResume2]

    input {
        resume #"
            Sam Lijin
            he/him |  jobs@sxlijin.com |  sxlijin.github.io |  sxlijin |  123-562-8967

            Experience
            Trunk
            | July 2021 - current
            Trunk Check | Senior Software Engineer | Services TL, Mar 2023 - current | IC, July 2021 - Feb 2023
            Proposed, designed, and led a team of 3 to build a web experience for Check (both a web-only onboarding flow and SaaS offerings)
            Proposed and built vulnerability scanning into Check, enabling it to compete with security products such as Snyk
            Helped grow Check from <1K users to 90K+ users by focusing on product-led growth
            Google | Sept 2017 - June 2021
            User Identity SRE | Senior Software Engineer | IC, Mar 2021 - June 2021
            Designed an incremental key rotation system to limit the global outage risk to Google SSO
            Discovered and severed an undocumented Gmail serving dependency on Identity-internal systems
            Cloud Firestore | Senior Software Engineer | EngProd TL, Aug 2019 - Feb 2021 | IC, Sept 2017 - July 2019
            Metadata TTL system: backlog of XX trillion records, sustained 1M ops/sec, peaking at 3M ops/sec

            Designed and implemented a logging system with novel observability and privacy requirements
            Designed and implemented Jepsen-style testing to validate correctness guarantees
            Datastore Migration: zero downtime, xM RPS and xxPB of data over xM customers and 36 datacenters

            Designed composite index migration, queue processing migration, progressive rollout, fast rollback, and disk stockout mitigations; implemented transaction log replay, state transitions, and dark launch process
            Designed and implemented end-to-end correctness and performance testing
            Velocity improvements for 60-eng org

            Proposed and implemented automated rollbacks: got us out of a 3-month release freeze and prevented 5 outages over the next 6 months
            Proposed and implemented new development and release environments spanning 30+ microservices
            Incident response for API proxy rollback affecting every Google Cloud service

            Google App Engine Memcache | Software Engineer | EngProd TL, Apr 2019 - July 2019
            Proposed and led execution of test coverage improvement strategy for a new control plane: reduced rollbacks and ensured strong consistency of a distributed cache serving xxM QPS
            Designed and implemented automated performance regression testing for two critical serving paths
            Used to validate Google-wide rollout of AMD CPUs, by proving a 50p latency delta of <10µs
            Implemented on shared Borg (i.e. vulnerable to noisy neighbors) with <12% variance
            Miscellaneous | Sept 2017 - June 2021
            Redesigned the Noogler training on Google-internal storage technologies & trained 2500+ Nooglers
            Landed multiple google3-wide refactorings, each spanning xxK files (e.g. SWIG to CLIF)
            Education
            Vanderbilt University (Nashville, TN) | May 2017 | B.S. in Computer Science, Mathematics, and Political Science

            Stuyvesant HS (New York, NY) | 2013

            Skills
            C++, Java, Typescript, Javascript, Python, Bash; light experience with Rust, Golang, Scheme
            gRPC, Bazel, React, Linux
            Hobbies: climbing, skiing, photography
        "#
    }
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
        {{ ctx.output_format }}

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
        {{ ctx.output_format }}

        JSON:
    "#
}
