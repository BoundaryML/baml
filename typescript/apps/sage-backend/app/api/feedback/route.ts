import type { NextRequest } from 'next/server';
import { NextResponse } from 'next/server';
import { SendFeedbackRequestSchema } from '@baml/sage-interface';
import { SlackFeedbackLogger } from '@/lib/slack-api';
import { NotionLogger } from '@/lib/notion-api';

const slack = new SlackFeedbackLogger();
const notionLogger = new NotionLogger();

export async function POST(request: NextRequest) {
  try {
    const body = await request.json();

    const reqBody = SendFeedbackRequestSchema.safeParse(body);

    if (!reqBody.success) {
      return NextResponse.json(
        {
          error: 'Request does not match expected schema',
          details: reqBody.error,
          expectedSchema: SendFeedbackRequestSchema.toString(),
        },
        { status: 400 },
      );
    }

    const feedbackData = reqBody.data;

    // Deliberately do not await these, so that the request can return immediately.
    slack.sendFeedback(feedbackData).catch((error: Error) => {
      console.error('Failed to send feedback to Slack:', error);
    });
    notionLogger.updateFeedback(feedbackData).catch((error: Error) => {
      console.error('Failed to update feedback in Notion:', error);
    });

    return NextResponse.json({
      enqueued: true,
      message: 'Feedback received'
    });
  } catch (error) {
    console.error('Error in send-feedback API:', error);
    return NextResponse.json(
      { 
        enqueued: false,
        error: 'Internal server error',
        message: error instanceof Error ? error.message : 'Unknown error'
      },
      { status: 500 }
    );
  }
}