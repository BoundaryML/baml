class Resume {
    name string
    email string
    phone string
    address string
    summary string
    experience Experience[]
    education Education[]
}

class Experience {
    company string
    title string
    location string
    startDate string
    endDate string
    type ExperienceType
}

class Education {
    school string
    degree string
    fieldOfStudy string
    startDate string
    endDate string
}

enum ExperienceType {
    WORK
    EDUCATION
}


type AllResumes = Resume[]

function GetAllResumes() -> AllResumes {
    client "openai/gpt-4o-mini"
    prompt #"
        Get all resumes
    "#
}
