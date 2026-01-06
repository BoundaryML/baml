// Test file for generated BAML client
// This will be compiled against the generated baml_client module

mod baml_client;

use baml_client::types::*;
use baml_client::sync_client::B;

fn main() {
    println!("Test - baml_client module loaded successfully!");
}

#[cfg(test)]
mod tests {
    use super::*;
    use baml_client::{
        new_audio_from_url, new_image_from_url, new_pdf_from_url, new_video_from_url,
    };

    #[test]
    fn test_image_input() {
        // Create an image from URL
        let image_media = new_image_from_url(
            "https://drive.google.com/uc?id=1NhoSIIHYveygPytfCroGaAHwJ5agD5a6",
            None,
        );

        // Wrap in union type
        let media_union = Union4AudioOrImageOrPDFOrVideo::Image(image_media);

        // Call the BAML function
        let result = B.TestMediaInput
            .call(&media_union, "Analyze this image")
            .expect("Failed to call TestMediaInput with image");

        // Validate that we got a non-empty analysis
        assert!(
            !result.analysisText.is_empty(),
            "Expected analysis text to be non-empty"
        );
    }

    #[test]
    #[ignore] // TODO: Requires ClientRegistry support for specifying audio-capable model
    fn test_audio_input() {
        // Create audio from URL
        let audio_media =
            new_audio_from_url("https://download.samplelib.com/mp3/sample-3s.mp3", None);

        // Wrap in union type
        let media_union = Union4AudioOrImageOrPDFOrVideo::Audio(audio_media);

        // Note: This test requires ClientRegistry to set the model to "openai/gpt-4o-audio-preview"
        // When ClientRegistry is implemented, add:
        // let result = B.with_options(|opts| opts.with_client_registry(...))
        //     .TestMediaInput(&media_union, "This is music used for an intro")
        //     .expect("Failed to call TestMediaInput with audio");

        let result = B.TestMediaInput
            .call(&media_union, "This is music used for an intro")
            .expect("Failed to call TestMediaInput with audio");

        assert!(
            !result.analysisText.is_empty(),
            "Expected analysis text to be non-empty"
        );
    }

    #[test]
    fn test_pdf_input() {
        // Create PDF from URL
        let pdf_media = new_pdf_from_url(
            "https://example-files.online-convert.com/document/pdf/example.pdf",
            None,
        );

        // Wrap in union type
        let media_union = Union4AudioOrImageOrPDFOrVideo::PDF(pdf_media);

        // Call the BAML function
        let result = B.TestMediaInput
            .call(&media_union, "Analyze this PDF")
            .expect("Failed to call TestMediaInput with PDF");

        // Validate that we got a non-empty analysis
        assert!(
            !result.analysisText.is_empty(),
            "Expected analysis text to be non-empty"
        );
    }

    #[test]
    #[ignore] // TODO: Requires ClientRegistry support for specifying video-capable model
    fn test_video_input() {
        // Create video from URL (YouTube)
        let video_media = new_video_from_url("https://www.youtube.com/watch?v=1O0yazhqaxs", None);

        // Wrap in union type
        let media_union = Union4AudioOrImageOrPDFOrVideo::Video(video_media);

        // Note: This test requires ClientRegistry to set the model to "google-ai/gemini-2.5-flash"
        // When ClientRegistry is implemented, add:
        // let result = B.with_options(|opts| opts.with_client_registry(...))
        //     .TestMediaInput(&media_union, "Analyze this video")
        //     .expect("Failed to call TestMediaInput with video");

        let result = B.TestMediaInput
            .call(&media_union, "Analyze this video")
            .expect("Failed to call TestMediaInput with video");

        assert!(
            !result.analysisText.is_empty(),
            "Expected analysis text to be non-empty"
        );
    }

    #[test]
    fn test_image_array_input() {
        // Create two images from URLs
        let image1 = new_image_from_url(
            "https://drive.google.com/uc?id=1NhoSIIHYveygPytfCroGaAHwJ5agD5a6",
            None,
        );

        let image2 = new_image_from_url(
            "https://upload.wikimedia.org/wikipedia/commons/thumb/a/a7/React-icon.svg/1200px-React-icon.svg.png",
            None,
        );

        // Create array of images
        let image_array = vec![image1, image2];

        // Call the BAML function with image array
        let result = B.TestMediaArrayInputs
            .call(&image_array, "Analyze these images")
            .expect("Failed to call TestMediaArrayInputs");

        // Validate that media count is positive
        assert!(
            result.mediaCount > 0,
            "Expected media count to be positive, got {}",
            result.mediaCount
        );

        // Validate that we got a non-empty analysis
        assert!(
            !result.analysisText.is_empty(),
            "Expected analysis text to be non-empty"
        );
    }
}
