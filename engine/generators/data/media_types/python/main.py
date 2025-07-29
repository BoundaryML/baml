import asyncio
import sys
import baml_py
from baml_client import baml

async def test_image_input():
    print("Testing image input...")
    result = await baml.TestMediaInput(
        media=baml_py.Image.from_url(
            "https://upload.wikimedia.org/wikipedia/en/4/4d/Shrek_%28character%29.png"
        ),
        textInput="Analyze this image"
    )
    assert result.analysisText != "", "Expected analysis text to be non-empty"
    assert result.mediaType == "image", "Expected mediaType to be 'image'"
    assert result.hasContent is True, "Expected hasContent to be True"
    print("✓ Image input test passed")

async def test_audio_input():
    print("\nTesting audio input...")
    result = await baml.TestMediaInput(
        media=baml_py.Audio.from_url(
            "https://actions.google.com/sounds/v1/emergency/beeper_emergency_call.ogg"
        ),
        textInput="Analyze this audio"
    )
    assert result.analysisText != "", "Expected analysis text to be non-empty"
    assert result.mediaType == "audio", "Expected mediaType to be 'audio'"
    assert result.hasContent is True, "Expected hasContent to be True"
    print("✓ Audio input test passed")

async def test_pdf_input():
    print("\nTesting PDF input...")
    result = await baml.TestMediaInput(
        media=baml_py.Pdf.from_url(
            "https://www.w3.org/WAI/ER/tests/xhtml/testfiles/resources/pdf/dummy.pdf"
        ),
        textInput="Analyze this PDF"
    )
    assert result.analysisText != "", "Expected analysis text to be non-empty"
    assert result.mediaType == "pdf", "Expected mediaType to be 'pdf'"
    assert result.hasContent is True, "Expected hasContent to be True"
    print("✓ PDF input test passed")

async def test_video_input():
    print("\nTesting video input...")
    result = await baml.TestMediaInput(
        media=baml_py.Video.from_url(
            "https://www.youtube.com/watch?v=1O0yazhqaxs"
        ),
        textInput="Analyze this video"
    )
    assert result.analysisText != "", "Expected analysis text to be non-empty"
    assert result.mediaType == "video", "Expected mediaType to be 'video'"
    assert result.hasContent is True, "Expected hasContent to be True"
    print("✓ Video input test passed")

async def test_image_array_input():
    print("\nTesting image array input...")
    result = await baml.TestMediaArrayInputs(
        imageArray=[
            baml_py.Image.from_url("https://upload.wikimedia.org/wikipedia/en/4/4d/Shrek_%28character%29.png"),
            baml_py.Image.from_url("https://upload.wikimedia.org/wikipedia/commons/thumb/a/a7/React-icon.svg/1200px-React-icon.svg.png")
        ],
        textInput="Analyze these images"
    )
    assert result.mediaCount > 0, "Expected media count to be positive"
    assert len(result.mediaTypes) > 0, "Expected media types to be listed"
    assert result.analysisText != "", "Expected analysis text to be non-empty"
    print("✓ Image array input test passed")

async def main():
    # Run all tests in sequence
    tests = [
        test_image_input(),
        test_audio_input(),
        test_pdf_input(),
        test_video_input(),
        test_image_array_input()
    ]
    
    try:
        for test in tests:
            await test
        print("\n✅ All media type tests passed!")
    except Exception as e:
        print(f"\n❌ Test failed: {e}")
        sys.exit(1)

if __name__ == "__main__":
    asyncio.run(main())