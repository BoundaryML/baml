FROM python:3.10-slim@sha256:fd76ade0c607f27677bc04be3c60749f400eedc941d9e72967e19a4cedff80c2
WORKDIR /app
ENV PYTHONUNBUFFERED=1 PYTHONDONTWRITEBYTECODE=1 PYTHONTRACEMALLOC=1
COPY .build/python-baseline/wheels /wheels
COPY python-baseline/requirements.txt ./
RUN pip install --no-cache-dir --no-index --find-links=/wheels --only-binary=:all: -r requirements.txt && rm -rf /wheels
COPY python-baseline/runtime_metrics.py python-baseline/server.py ./
EXPOSE 8080 9091
CMD ["uvicorn", "server:app", "--host", "0.0.0.0", "--port", "8080", "--no-access-log", "--no-server-header", "--timeout-keep-alive", "60"]
