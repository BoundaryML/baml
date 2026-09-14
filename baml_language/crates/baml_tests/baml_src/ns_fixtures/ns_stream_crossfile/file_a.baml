// File A: defines Education class and Degree enum.
// These are referenced by Resume in file_b.baml.

class Education {
  school string
  degree Degree
  year int
}

enum Degree {
  Bachelors
  Masters
  PhD
}

type EducationList = Education[]
