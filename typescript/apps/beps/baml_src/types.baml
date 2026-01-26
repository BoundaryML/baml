// Input types for version analysis

class VersionContent {
  version int
  title string
  content string
  editedBy string
  editNote string?
}

class FeedbackItem {
  id string
  type string @description("comment, concern, question, issue, or decision")
  content string
  author string
  resolved bool
  resolution string?
}

class VersionAnalysisInput {
  bepNumber int
  bepTitle string
  previousVersion VersionContent
  newVersion VersionContent
  feedbackItems FeedbackItem[]
}

// Output types for analysis result

enum FeedbackStatus {
  Addressed @description("The feedback was clearly addressed in the new version")
  PartiallyAddressed @description("Some aspects addressed, others remain")
  NotAddressed @description("The feedback was not addressed")
  NoLongerRelevant @description("The feedback is no longer applicable")
}

enum Verdict {
  Good @description("Author has adequately addressed the feedback")
  NeedsWork @description("Significant feedback remains unaddressed")
  Excellent @description("All feedback addressed thoroughly")
}

class FeedbackAnalysis {
  feedbackId string
  feedbackType string
  originalContent string
  status FeedbackStatus
  evidence string @description("Quote or reference from new version showing how it was addressed")
  explanation string
}

class Recommendation {
  priority string @description("high, medium, or low")
  description string
  relatedFeedbackIds string[]
}

class VersionAnalysisResult {
  addressedFeedback FeedbackAnalysis[]
  unaddressedFeedback FeedbackAnalysis[]
  partiallyAddressedFeedback FeedbackAnalysis[]
  overallVerdict Verdict
  verdictExplanation string
  recommendations Recommendation[]
  summary string @description("2-3 sentence executive summary")
}
