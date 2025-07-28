import { baml } from "../baml_client";
import { Image, Audio, Video, Pdf } from "../baml_client/types";

async function testImageInput(): Promise<void> {
  console.log("Testing image input...");
  const result = await baml.TestMediaInput(
    Image.fromUrl(
      "https://upload.wikimedia.org/wikipedia/en/4/4d/Shrek_%28character%29.png"
    ),
    "Analyze this image"
  );
  if (result.analysisText === "") {
    throw new Error("Expected analysis text to be non-empty");
  }
  if (result.mediaType !== "image") {
    throw new Error("Expected mediaType to be 'image'");
  }
  if (!result.hasContent) {
    throw new Error("Expected hasContent to be true");
  }
  console.log("✓ Image input test passed");
}

async function testAudioInput(): Promise<void> {
  console.log("\nTesting audio input...");
  const result = await baml.TestMediaInput(
    Audio.fromUrl(
      "https://actions.google.com/sounds/v1/emergency/beeper_emergency_call.ogg"
    ),
    "Analyze this audio"
  );
  if (result.analysisText === "") {
    throw new Error("Expected analysis text to be non-empty");
  }
  if (result.mediaType !== "audio") {
    throw new Error("Expected mediaType to be 'audio'");
  }
  if (!result.hasContent) {
    throw new Error("Expected hasContent to be true");
  }
  console.log("✓ Audio input test passed");
}

async function testPdfInput(): Promise<void> {
  console.log("\nTesting PDF input...");
  const result = await baml.TestMediaInput(
    Pdf.fromUrl(
      "https://www.w3.org/WAI/ER/tests/xhtml/testfiles/resources/pdf/dummy.pdf"
    ),
    "Analyze this PDF"
  );
  if (result.analysisText === "") {
    throw new Error("Expected analysis text to be non-empty");
  }
  if (result.mediaType !== "pdf") {
    throw new Error("Expected mediaType to be 'pdf'");
  }
  if (!result.hasContent) {
    throw new Error("Expected hasContent to be true");
  }
  console.log("✓ PDF input test passed");
}

async function testVideoInput(): Promise<void> {
  console.log("\nTesting video input...");
  const result = await baml.TestMediaInput(
    Video.fromUrl("https://www.youtube.com/watch?v=1O0yazhqaxs"),
    "Analyze this video"
  );
  if (result.analysisText === "") {
    throw new Error("Expected analysis text to be non-empty");
  }
  if (result.mediaType !== "video") {
    throw new Error("Expected mediaType to be 'video'");
  }
  if (!result.hasContent) {
    throw new Error("Expected hasContent to be true");
  }
  console.log("✓ Video input test passed");
}

async function testImageArrayInput(): Promise<void> {
  console.log("\nTesting image array input...");
  const result = await baml.TestMediaArrayInputs(
    [
      Image.fromUrl(
        "https://upload.wikimedia.org/wikipedia/en/4/4d/Shrek_%28character%29.png"
      ),
      Image.fromUrl(
        "https://upload.wikimedia.org/wikipedia/commons/thumb/a/a7/React-icon.svg/1200px-React-icon.svg.png"
      ),
    ],
    "Analyze these images"
  );
  if (result.mediaCount <= 0) {
    throw new Error("Expected media count to be positive");
  }
  if (result.mediaTypes.length === 0) {
    throw new Error("Expected media types to be listed");
  }
  if (result.analysisText === "") {
    throw new Error("Expected analysis text to be non-empty");
  }
  console.log("✓ Image array input test passed");
}

async function main(): Promise<void> {
  // Run all tests in sequence
  const tests = [
    testImageInput,
    testAudioInput,
    testPdfInput,
    testVideoInput,
    testImageArrayInput,
  ];

  try {
    for (const test of tests) {
      await test();
    }
    console.log("\n✅ All media type tests passed!");
  } catch (error) {
    console.error(`\n❌ Test failed: ${error}`);
    process.exit(1);
  }
}

// Run the tests
main();
