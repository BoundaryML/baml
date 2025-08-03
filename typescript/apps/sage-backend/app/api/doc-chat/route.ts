import type { NextRequest } from 'next/server';
import { NextResponse } from 'next/server';
import { submitQuery } from '../../actions/query';
import { QueryRequestSchema } from '../../types';

export async function POST(request: NextRequest) {
  try {
    const body = await request.json();

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

    const result = await submitQuery(validationResult.data);
    return NextResponse.json(result);
  } catch (error) {
    console.error('Error in doc-chat API:', error);
    return NextResponse.json(
      { error: 'Internal server error' },
      { status: 500 },
    );
  }
}
