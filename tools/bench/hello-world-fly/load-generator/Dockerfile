FROM python:3.10-slim@sha256:8c97ebedc32fd60935cdf5992e935753e2a0f98231830028050e1e04bd3c13c2
RUN apt-get update && apt-get install -y --no-install-recommends curl ca-certificates && rm -rf /var/lib/apt/lists/*
ARG VEGETA_VERSION=12.13.0
RUN curl -fsSL --retry 3 https://github.com/tsenart/vegeta/releases/download/v${VEGETA_VERSION}/vegeta_${VEGETA_VERSION}_linux_amd64.tar.gz -o /tmp/vegeta_${VEGETA_VERSION}_linux_amd64.tar.gz && curl -fsSL --retry 3 https://github.com/tsenart/vegeta/releases/download/v${VEGETA_VERSION}/vegeta_${VEGETA_VERSION}_checksums.txt -o /tmp/checksums && cd /tmp && grep "vegeta_${VEGETA_VERSION}_linux_amd64.tar.gz" checksums | sha256sum -c - && tar -xzf vegeta_${VEGETA_VERSION}_linux_amd64.tar.gz -C /usr/local/bin vegeta && vegeta -version
WORKDIR /app
COPY targets.json run.py manager.py entrypoint.sh ./
ENV PYTHONUNBUFFERED=1 DURATION_SECONDS=0
ENTRYPOINT ["sh", "/app/entrypoint.sh"]
