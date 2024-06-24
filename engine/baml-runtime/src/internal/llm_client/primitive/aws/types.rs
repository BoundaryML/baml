use aws_sdk_bedrockruntime::{self as bedrock};
use serde::Deserialize;

#[derive(Deserialize)]
/// Used to extract options.inference_configuration from the BAML client
/// We can't use #[serde(remote="bedrock::types::InferenceConfiguration")] because it's non-exhaustive
pub(super) struct InferenceConfiguration {
    /// <p>The maximum number of tokens to allow in the generated response. The default value is the maximum allowed value for the model that you are using. For more information, see <a href="https://docs.aws.amazon.com/bedrock/latest/userguide/model-parameters.html">Inference parameters for foundation models</a>.</p>
    max_tokens: ::std::option::Option<i32>,
    /// <p>The likelihood of the model selecting higher-probability options while generating a response. A lower value makes the model more likely to choose higher-probability options, while a higher value makes the model more likely to choose lower-probability options.</p>
    /// <p>The default value is the default value for the model that you are using. For more information, see <a href="https://docs.aws.amazon.com/bedrock/latest/userguide/model-parameters.html">Inference parameters for foundation models</a>.</p>
    temperature: ::std::option::Option<f32>,
    /// <p>The percentage of most-likely candidates that the model considers for the next token. For example, if you choose a value of 0.8 for <code>topP</code>, the model selects from the top 80% of the probability distribution of tokens that could be next in the sequence.</p>
    /// <p>The default value is the default value for the model that you are using. For more information, see <a href="https://docs.aws.amazon.com/bedrock/latest/userguide/model-parameters.html">Inference parameters for foundation models</a>.</p>
    top_p: ::std::option::Option<f32>,
    /// <p>A list of stop sequences. A stop sequence is a sequence of characters that causes the model to stop generating the response.</p>
    stop_sequences: ::std::option::Option<::std::vec::Vec<::std::string::String>>,
}

impl Into<bedrock::types::InferenceConfiguration> for InferenceConfiguration {
    fn into(self) -> bedrock::types::InferenceConfiguration {
        bedrock::types::InferenceConfiguration::builder()
            .set_max_tokens(self.max_tokens)
            .set_temperature(self.temperature)
            .set_top_p(self.top_p)
            .set_stop_sequences(self.stop_sequences)
            .build()
    }
}
