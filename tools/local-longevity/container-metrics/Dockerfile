FROM python:3.10-slim@sha256:fd76ade0c607f27677bc04be3c60749f400eedc941d9e72967e19a4cedff80c2
WORKDIR /app
ENV PYTHONUNBUFFERED=1
COPY container-metrics/run.py ./
EXPOSE 9090
CMD ["python", "run.py"]
