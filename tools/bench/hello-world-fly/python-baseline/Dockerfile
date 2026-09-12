FROM python:3.10-slim@sha256:8c97ebedc32fd60935cdf5992e935753e2a0f98231830028050e1e04bd3c13c2
WORKDIR /app
ENV PYTHONUNBUFFERED=1 PYTHONDONTWRITEBYTECODE=1 PYTHONTRACEMALLOC=1
COPY requirements.txt ./
RUN pip install --no-cache-dir -r requirements.txt
COPY runtime_metrics.py server.py ./
RUN python -c "from runtime_metrics import render_metrics; read=lambda:dict(l.split() for l in render_metrics().decode().splitlines() if l and not l.startswith('#')); before=read(); held=bytearray(1048576); after=read(); assert float(after['process_resident_memory_bytes'])>0; assert after['python_tracemalloc_enabled']=='1'; assert float(after['python_traced_memory_current_bytes'])-float(before['python_traced_memory_current_bytes'])>=1048576"
EXPOSE 8080
CMD ["uvicorn", "server:app", "--host", "0.0.0.0", "--port", "8080", "--no-access-log", "--no-server-header", "--timeout-keep-alive", "60"]
