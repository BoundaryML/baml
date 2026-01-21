from __future__ import annotations

import json
import os
import urllib.parse
import urllib.request
from dataclasses import dataclass
from typing import Any, Optional


_DEFAULT_BASE_URL = "https://generativelanguage.googleapis.com"


@dataclass(frozen=True)
class GeminiFile:
    name: str
    uri: str
    mime_type: Optional[str] = None
    expiration_time: Optional[str] = None
    display_name: Optional[str] = None


@dataclass(frozen=True)
class GeminiCachedContent:
    name: str
    model: str
    expire_time: Optional[str] = None
    display_name: Optional[str] = None


class GoogleAIRequestError(RuntimeError):
    pass


def _api_key_or_env(api_key: Optional[str]) -> str:
    api_key = (api_key or "").strip()
    if api_key:
        return api_key
    env = (os.getenv("GOOGLE_API_KEY") or "").strip()
    if not env:
        raise GoogleAIRequestError("Missing Google AI API key (set GOOGLE_API_KEY).")
    return env


def _url_with_key(base_url: str, path: str, api_key: str) -> str:
    base_url = base_url.rstrip("/")
    path = "/" + path.lstrip("/")
    query = urllib.parse.urlencode({"key": api_key})
    return f"{base_url}{path}?{query}"


def _http_json(
    *,
    method: str,
    url: str,
    headers: dict[str, str],
    body: Optional[bytes] = None,
    timeout_seconds: float = 60.0,
) -> dict[str, Any]:
    request = urllib.request.Request(url, data=body, method=method)
    for k, v in headers.items():
        request.add_header(k, v)
    try:
        with urllib.request.urlopen(request, timeout=timeout_seconds) as resp:  # noqa: S310
            payload = resp.read()
            if not payload:
                return {}
            return json.loads(payload.decode("utf-8"))
    except urllib.error.HTTPError as exc:  # noqa: PERF203
        raw = exc.read()
        msg = raw.decode("utf-8", errors="replace") if raw else str(exc)
        raise GoogleAIRequestError(f"Google AI request failed: {exc.code} {msg}") from exc


def upload_file_bytes(
    *,
    data: bytes,
    mime_type: str,
    display_name: Optional[str] = None,
    api_key: Optional[str] = None,
    base_url: str = _DEFAULT_BASE_URL,
    timeout_seconds: float = 120.0,
) -> GeminiFile:
    """
    Upload bytes to Gemini Files API and return the resulting file handle.

    Uses the resumable upload flow documented for:
      POST /upload/v1beta/files
    """
    api_key = _api_key_or_env(api_key)
    num_bytes = len(data)
    if num_bytes <= 0:
        raise ValueError("upload_file_bytes: data is empty")

    start_url = _url_with_key(base_url, "/upload/v1beta/files", api_key)
    start_headers = {
        "X-Goog-Upload-Protocol": "resumable",
        "X-Goog-Upload-Command": "start",
        "X-Goog-Upload-Header-Content-Length": str(num_bytes),
        "X-Goog-Upload-Header-Content-Type": mime_type,
        "Content-Type": "application/json",
    }
    start_body = json.dumps(
        {"file": {"display_name": (display_name or "")}}, ensure_ascii=False
    ).encode("utf-8")

    start_req = urllib.request.Request(start_url, data=start_body, method="POST")
    for k, v in start_headers.items():
        start_req.add_header(k, v)

    try:
        with urllib.request.urlopen(start_req, timeout=timeout_seconds) as resp:  # noqa: S310
            upload_url = resp.headers.get("x-goog-upload-url")
    except urllib.error.HTTPError as exc:  # noqa: PERF203
        raw = exc.read()
        msg = raw.decode("utf-8", errors="replace") if raw else str(exc)
        raise GoogleAIRequestError(f"Files upload init failed: {exc.code} {msg}") from exc

    if not upload_url:
        raise GoogleAIRequestError("Files upload init failed: missing x-goog-upload-url")

    finalize_headers = {
        "Content-Length": str(num_bytes),
        "X-Goog-Upload-Offset": "0",
        "X-Goog-Upload-Command": "upload, finalize",
    }
    request = urllib.request.Request(upload_url, data=data, method="POST")
    for k, v in finalize_headers.items():
        request.add_header(k, v)

    try:
        with urllib.request.urlopen(request, timeout=timeout_seconds) as resp:  # noqa: S310
            payload = resp.read().decode("utf-8")
            info = json.loads(payload) if payload else {}
    except urllib.error.HTTPError as exc:  # noqa: PERF203
        raw = exc.read()
        msg = raw.decode("utf-8", errors="replace") if raw else str(exc)
        raise GoogleAIRequestError(f"Files upload finalize failed: {exc.code} {msg}") from exc

    file_obj = info.get("file") or {}
    if not isinstance(file_obj, dict):
        raise GoogleAIRequestError("Files upload finalize failed: invalid response payload")

    return GeminiFile(
        name=str(file_obj.get("name") or ""),
        uri=str(file_obj.get("uri") or ""),
        mime_type=(file_obj.get("mimeType") if isinstance(file_obj.get("mimeType"), str) else None),
        expiration_time=(
            file_obj.get("expirationTime")
            if isinstance(file_obj.get("expirationTime"), str)
            else None
        ),
        display_name=(
            file_obj.get("displayName") if isinstance(file_obj.get("displayName"), str) else None
        ),
    )


def create_cached_content(
    *,
    model: str,
    system_instruction: str,
    file_uri: str,
    file_mime_type: str,
    ttl_seconds: int = 3600,
    display_name: Optional[str] = None,
    api_key: Optional[str] = None,
    base_url: str = _DEFAULT_BASE_URL,
    timeout_seconds: float = 60.0,
) -> GeminiCachedContent:
    """
    Create explicit cached content that includes a file + stable system instruction.

    The resulting `name` is suitable for `cachedContent` in generateContent requests.
    """
    api_key = _api_key_or_env(api_key)
    if not model.startswith("models/"):
        model = f"models/{model}"

    ttl_seconds = int(ttl_seconds)
    if ttl_seconds <= 0:
        raise ValueError("ttl_seconds must be > 0")

    url = _url_with_key(base_url, "/v1beta/cachedContents", api_key)
    body_obj: dict[str, Any] = {
        "model": model,
        "ttl": f"{ttl_seconds}s",
        "systemInstruction": {
            "role": "system",
            "parts": [{"text": system_instruction}],
        },
        "contents": [
            {
                "role": "user",
                "parts": [
                    {
                        "fileData": {
                            "fileUri": file_uri,
                            "mimeType": file_mime_type,
                        }
                    }
                ],
            }
        ],
    }
    if display_name:
        body_obj["displayName"] = display_name

    resp = _http_json(
        method="POST",
        url=url,
        headers={"Content-Type": "application/json"},
        body=json.dumps(body_obj, ensure_ascii=False).encode("utf-8"),
        timeout_seconds=timeout_seconds,
    )

    name = resp.get("name")
    if not isinstance(name, str) or not name:
        raise GoogleAIRequestError("Create cached content failed: missing name")

    return GeminiCachedContent(
        name=name,
        model=str(resp.get("model") or model),
        expire_time=(resp.get("expireTime") if isinstance(resp.get("expireTime"), str) else None),
        display_name=(
            resp.get("displayName") if isinstance(resp.get("displayName"), str) else None
        ),
    )
