FROM python:3.10-slim@sha256:fd76ade0c607f27677bc04be3c60749f400eedc941d9e72967e19a4cedff80c2 AS build
RUN apt-get update && apt-get install -y --no-install-recommends curl ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
ENV BAML_PROFILE=0 BAML_TELEMETRY_DISABLED=1 BAML_LOG=off
COPY scripts/install-baml.sh apps/python-baml/baml.toml ./
COPY apps/python-baml/baml_src ./baml_src
ARG TARGETARCH
RUN sh install-baml.sh "$TARGETARCH" gnu && /opt/baml/bin/baml-cli generate --agent-skill-check off
FROM python:3.10-slim@sha256:fd76ade0c607f27677bc04be3c60749f400eedc941d9e72967e19a4cedff80c2
WORKDIR /app
ENV BAML_PROFILE=0 BAML_TELEMETRY_DISABLED=1 BAML_LOG=off
ENV PYTHONUNBUFFERED=1 PYTHONDONTWRITEBYTECODE=1 PYTHONTRACEMALLOC=1
COPY apps/python-baml/requirements.txt ./
RUN pip install --no-cache-dir -r requirements.txt
COPY --from=build /app/baml_sdk ./baml_sdk
COPY apps/python-baml/runtime_metrics.py apps/python-baml/server.py ./
RUN python -c "import asyncio; from baml_sdk import hello_world_async; assert asyncio.run(hello_world_async()) == 'hello world'"
RUN python -c "from runtime_metrics import render_metrics; read=lambda:dict(l.split() for l in render_metrics().decode().splitlines() if l and not l.startswith('#')); before=read(); held=bytearray(1048576); after=read(); assert float(after['process_resident_memory_bytes'])>0; assert after['python_tracemalloc_enabled']=='1'; assert float(after['python_traced_memory_current_bytes'])-float(before['python_traced_memory_current_bytes'])>=1048576"
EXPOSE 8080
CMD ["uvicorn", "server:app", "--host", "0.0.0.0", "--port", "8080", "--no-access-log", "--no-server-header", "--timeout-keep-alive", "60"]
