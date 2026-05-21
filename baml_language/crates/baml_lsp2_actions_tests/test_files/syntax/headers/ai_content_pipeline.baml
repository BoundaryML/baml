//# AI Content Pipeline
function AIContentPipeline(video_id: string, zoom_meeting_id: string, transcript: string?, title: string?) -> string {
    //# Fetch Resources

    //## Retrieve Emails and Posts from Database
    let emails = GetEmails();
    let posts = GetPosts();

    //## Process and Load Video
    let processed = ProcessVideo(video_id, zoom_meeting_id);
    let video = GetVideo(video_id);


    //# Generate Content

    //## Video Summarization
    let summary = if (transcript) {
        //### Generate Summary

        //#### Create Summary Text
        let s = SummarizeVideo(transcript, title);

        //#### Label Summary Category
        let category = LabelSummaryCategory(s);

        //### Save Summary
        let saved_summary = SaveSummary(video_id, s);

        saved_summary
    } else {
        //### Skip Summarization
        "no-transcript"
    };

    let content_types = ["email", "x", "linkedin"];

    //## Generate Drafts for Posts
    for (let kind in content_types) {
        //### Get Content Type and Generate Draft
        let structure = GetStructure(kind);
        let draft = GenerateContent(kind, video_id, summary);

        //### Save Draft
        let saved_draft = SaveDraft(video_id, draft);

        saved_draft
    }

    //# Prepare Pull Request
    let pr_status = if (summary) {
        //### Create GitHub PR
        let pr = CreatePR(video_id, summary);
        pr_status
    } else {
        //### Missing Summary
        "no-pr"
    };

    //# Complete
    "done"
}

//----
//- diagnostics
// error: unresolved name: GetEmails
//    ╭─[ headers_ai_content_pipeline.baml:6:18 ]
//    │
//  6 │     let emails = GetEmails();
//    │                  ────┬────  
//    │                      ╰────── unresolved name: GetEmails
//    │ 
//    │ Note: Error code: E0003
// ───╯
// error: unresolved name: GetPosts
//    ╭─[ headers_ai_content_pipeline.baml:7:17 ]
//    │
//  7 │     let posts = GetPosts();
//    │                 ────┬───  
//    │                     ╰───── unresolved name: GetPosts
//    │ 
//    │ Note: Error code: E0003
// ───╯
// error: unresolved name: ProcessVideo
//     ╭─[ headers_ai_content_pipeline.baml:10:21 ]
//     │
//  10 │     let processed = ProcessVideo(video_id, zoom_meeting_id);
//     │                     ──────┬─────  
//     │                           ╰─────── unresolved name: ProcessVideo
//     │ 
//     │ Note: Error code: E0003
// ────╯
// error: unresolved name: GetVideo
//     ╭─[ headers_ai_content_pipeline.baml:11:17 ]
//     │
//  11 │     let video = GetVideo(video_id);
//     │                 ────┬───  
//     │                     ╰───── unresolved name: GetVideo
//     │ 
//     │ Note: Error code: E0003
// ────╯
// error: unresolved name: SummarizeVideo
//     ╭─[ headers_ai_content_pipeline.baml:21:17 ]
//     │
//  21 │         let s = SummarizeVideo(transcript, title);
//     │                 ───────┬──────  
//     │                        ╰──────── unresolved name: SummarizeVideo
//     │ 
//     │ Note: Error code: E0003
// ────╯
// error: unresolved name: LabelSummaryCategory
//     ╭─[ headers_ai_content_pipeline.baml:24:24 ]
//     │
//  24 │         let category = LabelSummaryCategory(s);
//     │                        ──────────┬─────────  
//     │                                  ╰─────────── unresolved name: LabelSummaryCategory
//     │ 
//     │ Note: Error code: E0003
// ────╯
// error: unresolved name: SaveSummary
//     ╭─[ headers_ai_content_pipeline.baml:27:29 ]
//     │
//  27 │         let saved_summary = SaveSummary(video_id, s);
//     │                             ─────┬─────  
//     │                                  ╰─────── unresolved name: SaveSummary
//     │ 
//     │ Note: Error code: E0003
// ────╯
// error: unresolved name: GetStructure
//     ╭─[ headers_ai_content_pipeline.baml:40:25 ]
//     │
//  40 │         let structure = GetStructure(kind);
//     │                         ──────┬─────  
//     │                               ╰─────── unresolved name: GetStructure
//     │ 
//     │ Note: Error code: E0003
// ────╯
// error: unresolved name: GenerateContent
//     ╭─[ headers_ai_content_pipeline.baml:41:21 ]
//     │
//  41 │         let draft = GenerateContent(kind, video_id, summary);
//     │                     ───────┬───────  
//     │                            ╰───────── unresolved name: GenerateContent
//     │ 
//     │ Note: Error code: E0003
// ────╯
// error: unresolved name: SaveDraft
//     ╭─[ headers_ai_content_pipeline.baml:44:27 ]
//     │
//  44 │         let saved_draft = SaveDraft(video_id, draft);
//     │                           ────┬────  
//     │                               ╰────── unresolved name: SaveDraft
//     │ 
//     │ Note: Error code: E0003
// ────╯
// error: unresolved name: CreatePR
//     ╭─[ headers_ai_content_pipeline.baml:52:18 ]
//     │
//  52 │         let pr = CreatePR(video_id, summary);
//     │                  ────┬───  
//     │                      ╰───── unresolved name: CreatePR
//     │ 
//     │ Note: Error code: E0003
// ────╯
// error: unresolved name: pr_status
//     ╭─[ headers_ai_content_pipeline.baml:53:9 ]
//     │
//  53 │         pr_status
//     │         ────┬────  
//     │             ╰────── unresolved name: pr_status
//     │ 
//     │ Note: Error code: E0003
// ────╯
