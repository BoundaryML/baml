FROM python:3.10-slim@sha256:fd76ade0c607f27677bc04be3c60749f400eedc941d9e72967e19a4cedff80c2
RUN apt-get update && apt-get install -y --no-install-recommends curl ca-certificates && rm -rf /var/lib/apt/lists/*
ARG VEGETA_VERSION=12.13.0
RUN curl -fsSL --retry 3 https://github.com/tsenart/vegeta/releases/download/v${VEGETA_VERSION}/vegeta_${VEGETA_VERSION}_linux_amd64.tar.gz -o /tmp/vegeta_${VEGETA_VERSION}_linux_amd64.tar.gz && curl -fsSL --retry 3 https://github.com/tsenart/vegeta/releases/download/v${VEGETA_VERSION}/vegeta_${VEGETA_VERSION}_checksums.txt -o /tmp/checksums && cd /tmp && grep "vegeta_${VEGETA_VERSION}_linux_amd64.tar.gz" checksums | sha256sum -c - && tar -xzf vegeta_${VEGETA_VERSION}_linux_amd64.tar.gz -C /usr/local/bin vegeta && vegeta -version
WORKDIR /app
COPY load-generator/run.py ./
ENV PYTHONUNBUFFERED=1 GOMAXPROCS=1
ENTRYPOINT ["python", "/app/run.py"]
