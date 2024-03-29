# Run a python test script
FROM python:3.8-slim
WORKDIR /myapp

# Install pipenv
RUN pip install pipenv

# Set up environment variables for testing
ENV CAPTURE_DIR="/usr/src/logs"
RUN mkdir -p /usr/src/logs

# Copy the test script into the image
COPY script.sh /usr/src/test.sh
RUN chmod +x /usr/src/test.sh

# Install baml
RUN pipenv install baml

CMD ["/usr/src/test.sh"]
