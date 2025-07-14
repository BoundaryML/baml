class Issue {
  number int
  title string
  body string?
  type "pull_request" | "issue"
}

function FindRelatedIssue(message: string, issues: Issue[]) -> int? {
  client ClaudeSonnet
  prompt #"
    {{ _.role('system') }} We are developing a language called BAML and users
    give us feedback in a community chat.
    
    {{ _.role('user') }} You are given a message from a user and a list of
    issues from our GitHub repository. You have to find the issue or pull
    request that is the most related to the message and return its issue number.
    If no issue is related to the message, return null. If both a matching issue
    and pull request is found, then return the pull request number.
    
    MESSAGE:
    {{ message }}
    
    ISSUES:
    {{ issues }}

    Answer with the issue number or null without any additional explanation.
  "#
}

test FindRelatedIssueTest {
  functions [FindRelatedIssue]
  args {
    message "Can I use literal types in BAML?"
    issues [
      {
        number 1
        title "Support literal types"
        body "Add support for literal types in BAML."
        type "issue"
        labels ["feature"]
      }
      {
        number 2
        title "Add support for recursive types"
        body "Recursive types are working!"
        type "pull_request"
        labels ["feature"]
      }
    ]
  }
}
