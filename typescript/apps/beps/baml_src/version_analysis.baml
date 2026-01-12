function AnalyzeVersionChanges(input: VersionAnalysisInput) -> VersionAnalysisResult {
  client ClaudeSonnet
  prompt #"
    You are analyzing changes between two versions of a BAML Enhancement Proposal (BEP) to determine whether the author has adequately addressed community feedback.

    ## BEP Information
    **BEP-{{ input.bepNumber }}: {{ input.bepTitle }}**

    ## Previous Version (v{{ input.previousVersion.version }})
    Edited by: {{ input.previousVersion.editedBy }}
    {% if input.previousVersion.editNote %}Edit note: {{ input.previousVersion.editNote }}{% endif %}

    Content:
    {{ input.previousVersion.content }}

    ## New Version (v{{ input.newVersion.version }})
    Edited by: {{ input.newVersion.editedBy }}
    {% if input.newVersion.editNote %}Edit note: {{ input.newVersion.editNote }}{% endif %}

    Content:
    {{ input.newVersion.content }}

    ## Feedback to Analyze

    {% for item in input.feedbackItems %}
    ### {{ item.type | upper }} (ID: {{ item.id }})
    From: {{ item.author }}
    Status: {% if item.resolved %}Resolved{% else %}Open{% endif %}
    {% if item.resolution %}Resolution note: {{ item.resolution }}{% endif %}

    {{ item.content }}

    ---
    {% endfor %}

    ## Your Task

    Analyze the new version to determine how well it addresses each piece of feedback. For each feedback item:

    1. **Search for evidence** in the new version that shows the feedback was addressed
    2. **Determine the status**: Addressed, PartiallyAddressed, NotAddressed, or NoLongerRelevant
    3. **Provide specific evidence** by quoting relevant passages from the new version
    4. **Explain your reasoning** briefly

    Then provide:
    - An **overall verdict** (Excellent, Good, or NeedsWork)
    - **Recommendations** for the author, prioritized by importance
    - A brief **summary** of the analysis

    Be fair but thorough. Authors should be given credit for genuine attempts to address feedback, but significant gaps should be called out.

    {{ ctx.output_format }}
  "#
}
