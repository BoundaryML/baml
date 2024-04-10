enum ReviewSentiment {
  @@alias(sentiment)
  
  Happy
  Neutral
  Sad
}

enum ReviewHelpfulness {
  @@alias(helpful)
  
  VeryHelpful
  @alias(very helpful) 

  SomewhatHelpful
  @alias(somewhat helpful) 

  NotHelpful
  @alias(not helpful)
}

function AnalyzeProductReview(product_review: string) -> ReviewAnalysis {
    client AZURE_GPT4

    prompt #"
        {{ _.chat("system")}}

        You are a customer feedback analysis assistant.
        Your job is classify the sentiment of and determine how helpful product reviews are.

        {{ _.chat("user")}}

        Analyze the sentiment and helpfulness of the following product review:
        
        > {{ product_review }}
  
        {{ ctx.output_schema }}

        JSON:
    "#

}  

class ReviewAnalysis { 
  sentiment ReviewSentiment
  helpfulness ReviewHelpfulness
} 
