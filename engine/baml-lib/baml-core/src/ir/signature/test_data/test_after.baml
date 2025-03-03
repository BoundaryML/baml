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
    // fieldOfStudy string
    startDate string
    endDate string
}

enum ExperienceType {
    WORK
    EDUCATION
}

type JSON = string | float | int | bool | JSON[] | map<string, JSON> | null

class Article {
    title string
    content string
    tags Tag[]
    sub_articles Article[]
}

class Tag {
    name string
    articles Article[]
}

