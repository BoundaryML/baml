import { NotionLogger } from '@/lib/notion-api';
import { QueryRequestSchema } from '@baml/sage-interface';
import type { NextRequest } from 'next/server';
import { NextResponse } from 'next/server';
import { submitQuery } from '../../actions/query';

const notionLogger = new NotionLogger();

export async function POST(httpRequest: NextRequest) {
  try {
    const body = await httpRequest.json();

    // Validate the request body using the schema
    const validationResult = QueryRequestSchema.safeParse(body);

    if (!validationResult.success) {
      return NextResponse.json(
        {
          error: 'Request does not match expected schema',
          details: validationResult.error,
          expectedSchema: QueryRequestSchema.toString(),
        },
        { status: 400 },
      );
    }
    const request = validationResult.data;

    const result = await submitQuery(request);

    notionLogger
      .appendEntry({
        session_id: request.session_id,
        user_message: request.message,
        assistant_message: result.message,
      })
      .catch((error: Error) => {
        console.error('Failed to log chat to Notion:', error);
      });

    return NextResponse.json(result);
  } catch (error) {
    console.error('Error in doc-chat API:', error);
    return NextResponse.json({ error, message: 'Internal server error' }, { status: 500 });
  }
}
