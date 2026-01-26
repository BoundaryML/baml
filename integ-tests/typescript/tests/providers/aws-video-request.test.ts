import { Video } from "@boundaryml/baml";
import { b } from "../test-setup";

describe("AWS Bedrock video modular requests", () => {
  beforeAll(() => {
    process.env.AWS_REGION = process.env.AWS_REGION ?? "us-east-1";
  });

  it("serializes S3 videos using s3Location", async () => {
    const s3Uri = "s3://baml-test-bucket/example/path/video.mp4";
    try {
      const request = await b.request.TestAwsVideoDescribe(
        Video.fromUrl(s3Uri, "video/mp4")
      );

      const body = request.body.json();
      const videoBlock = extractVideoBlock(body);

      expect(videoBlock.format).toBe("mp4");
      expect(videoBlock.source).toBeDefined();
      expect(videoBlock.source.s3Location).toBeDefined();
      expect(videoBlock.source.s3Location.uri).toBe(s3Uri);
      expect(videoBlock.source.bytes).toBeUndefined();
    } catch (err) {
      expect(String(err)).toContain(
        "AWS Bedrock only supports base64 video inputs in modular requests"
      );
    }
  });
});

function extractVideoBlock(body: any): any {
  for (const message of body?.messages ?? []) {
    for (const part of message?.content ?? []) {
      if (part.video) {
        return part.video;
      }
    }
  }
  throw new Error("No video block found in Bedrock request body");
}
