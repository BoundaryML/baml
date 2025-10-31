#!/bin/bash

# Deploy Python Lambda to AWS
LAMBDA_NAME="sam-wif1"
ZIP_FILE="lambda_function.zip"
PACKAGE_DIR="package"

# Clean up previous build
rm -rf $PACKAGE_DIR $ZIP_FILE

# Install dependencies
echo "Installing dependencies..."
pip install -r requirements.txt -t $PACKAGE_DIR

# Copy Lambda function
cp lambda_function.py $PACKAGE_DIR/

# Create deployment package
echo "Creating deployment package..."
cd $PACKAGE_DIR
zip -r ../$ZIP_FILE .
cd ..

# Upload to Lambda
echo "Uploading to Lambda function: $LAMBDA_NAME..."
aws lambda update-function-code \
    --function-name $LAMBDA_NAME \
    --zip-file fileb://$ZIP_FILE

echo "Deployment complete!"

# Clean up
rm -rf $PACKAGE_DIR $ZIP_FILE
