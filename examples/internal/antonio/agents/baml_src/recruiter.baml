class CandidateInfo {
    name string
    is_technical bool
    specialty string
    resume_text string
}

function ExtractCandidateInfo(resume_text: string) -> CandidateInfo {
    client CustomGPT5Mini
    prompt #"
        You are a recruiter. Extract information from a resume text.

        {{ ctx.output_format }}

        Resume text:

        {{ resume_text }}
    "#
}

function AskFollowUpTechnicalQuestion(candidate: CandidateInfo) -> string {
    client CustomGPT5Mini
    prompt #"
        You are a technical recruiter. Craft a question for {{ candidate.name }}
        about how they got into {{ candidate.specialty }}.

        Candidate info:

        {{ candidate_info }}
    "#
}

function AskFollowUpBehavioralQuestion(candidate: CandidateInfo) -> string {
    client CustomGPT5Mini
    prompt #"
        You are a recruiter. Craft a question for {{ candidate.name }}
        asking what interests them about this role.

        Candidate info:

        {{ candidate_info }}
    "#
}

function Recruiter(resume_text: string) -> string {
    let candidate = ExtractCandidateInfo(resume_text);

    let question = if (candidate.is_technical) {
        AskFollowUpTechnicalQuestion(candidate)
    } else {
        AskFollowUpBehavioralQuestion(candidate)
    };

    question
}