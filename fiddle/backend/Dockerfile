## Use an official Ubuntu as a parent image
FROM python:latest

## Set the working directory in the container
WORKDIR /backend

# Copy the current directory contents into the container
COPY . .

# Make port 80 available to the world outside this container
EXPOSE 8000

# Define environment variable
# ENV NAME World
ENV BOUNDARY_PROJECT_ID placeholder
ENV BOUNDARY_SECRET placeholder
ENV OPENAI_API_KEY placeholder
ENV OPENAI_KEY placeholder

RUN pwd
RUN ls -al
RUN chmod u+x bootstrap.sh
RUN ./bootstrap.sh

CMD ["python3", "server.py"]
