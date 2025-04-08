export function formatError(error: any): {
  title: string;
  message: string;
  statusCode?: number;
  clientName?: string;
} {
  if (!error) return { title: 'No error', message: 'No error available' };

  try {
    // If error is a string, return it directly
    if (typeof error === 'string') {
      return { title: 'Error', message: error };
    }

    // Parse error if it's a string representation of JSON
    const errorObj = typeof error === 'string' ? JSON.parse(error) : error;

    // Extract the most relevant information
    const title = errorObj.name || 'Error';
    let message = errorObj.message || '';

    // If the message contains a nested error structure, try to extract the actual message
    if (message.includes('BamlError:')) {
      // Extract the actual error message from nested structure
      const matches = message.match(/message: Some\(\s*"([^"]+)"\s*\)/);
      if (matches?.[1]) {
        message = matches[1];
      }
    }

    console.error('Error', errorObj);
    return {
      clientName: errorObj.client_name,
      title,
      message,
      statusCode: errorObj.status_code,
    };
  } catch (e) {
    // Fallback for any parsing errors
    return {
      title: 'Error',
      message: String(error),
    };
  }
}
