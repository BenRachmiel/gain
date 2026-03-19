FROM python:3.12-slim
WORKDIR /app
COPY requirements.txt .
RUN pip install --no-cache-dir -r requirements.txt
COPY musiclib/ musiclib/
COPY tools/ tools/
USER 1000
ENTRYPOINT ["python", "-m"]
