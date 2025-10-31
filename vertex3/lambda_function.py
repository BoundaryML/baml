import json
import os
from vertexai.generative_models import GenerativeModel
import vertexai

# Initialize Vertex AI with project from environment variable
PROJECT_ID = os.environ['GCP_PROJECT_ID']
LOCATION = os.environ.get('GCP_LOCATION', 'us-central1')

def lambda_handler(event, context):
    """
    AWS Lambda handler that calls GCP Vertex AI Gemini 2.5 Flash
    """
    try:
        # Initialize Vertex AI
        vertexai.init(project=PROJECT_ID, location=LOCATION)

        # Create model instance
        model = GenerativeModel("gemini-2.0-flash-exp")

        # Get prompt from event or use default
        prompt = event.get('prompt', 'Hello, tell me about yourself!')

        # Generate response
        response = model.generate_content(prompt)

        return {
            'statusCode': 200,
            'body': json.dumps({
                'message': 'Success',
                'response': response.text,
                'model': 'gemini-2.0-flash-exp'
            })
        }
    except Exception as e:
        return {
            'statusCode': 500,
            'body': json.dumps({
                'error': str(e)
            })
        }
