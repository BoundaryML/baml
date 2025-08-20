import ctypes
import platform
import os
import sys
from pathlib import Path
from typing import Optional
import logging

# Constants matching Go implementation
VERSION = "0.205.0"
GITHUB_REPO = "boundaryml/baml"
BAML_CACHE_DIR_ENV_VAR = "BAML_CACHE_DIR"
BAML_LIBRARY_PATH_ENV = "BAML_LIBRARY_PATH"
BAML_DISABLE_DL_ENV = "BAML_LIBRARY_DISABLE_DOWNLOAD"

logger = logging.getLogger(__name__)

# Global library path (can be set before init)
_baml_shared_library_path = ""

def set_shared_library_path(path: str):
    """Set explicit library path (must be called before first load)"""
    global _baml_shared_library_path
    _baml_shared_library_path = path

def _get_cache_dir() -> Path:
    """Get cache directory matching Go's getCacheDir()"""
    cache_dir = os.environ.get(BAML_CACHE_DIR_ENV_VAR)
    
    if cache_dir:
        source = f"environment variable {BAML_CACHE_DIR_ENV_VAR}"
    else:
        # Python equivalent of os.UserCacheDir()
        if sys.platform == "darwin":
            cache_base = Path.home() / "Library" / "Caches"
        elif sys.platform.startswith("linux"):
            cache_base = Path(os.environ.get("XDG_CACHE_HOME", Path.home() / ".cache"))
        else:
            cache_base = Path(os.environ.get("LOCALAPPDATA", Path.home() / "AppData" / "Local"))
        
        cache_dir = cache_base / "baml" / "libs" / VERSION
        source = "default user cache location"
    
    cache_path = Path(cache_dir)
    logger.debug(f"Using cache directory from {source}: {cache_path}")
    
    # Create directory if needed
    cache_path.mkdir(parents=True, exist_ok=True)
    return cache_path

def _get_target_lib_filename() -> str:
    """Get platform-specific library filename matching Go's getTargetLibFilename()"""
    system = platform.system()
    arch = platform.machine()
    
    # Normalize architecture names to match Go
    if arch in ["x86_64", "AMD64"]:
        arch = "x86_64"
    elif arch in ["arm64", "aarch64"]:
        arch = "aarch64"
    
    lib_name = "libbaml_cffi"
    
    if system == "Darwin":
        ext = "dylib"
        if arch == "x86_64":
            target_triple = "x86_64-apple-darwin"
        elif arch == "aarch64":
            target_triple = "aarch64-apple-darwin"
        else:
            raise RuntimeError(f"Unsupported architecture: {arch}")
    elif system == "Linux":
        ext = "so"
        # TODO: Detect musl vs glibc
        is_musl = False
        if is_musl:
            suffix = "unknown-linux-musl"
        else:
            suffix = "unknown-linux-gnu"
        
        if arch == "x86_64":
            target_triple = f"x86_64-{suffix}"
        elif arch == "aarch64":
            target_triple = f"aarch64-{suffix}"
        else:
            raise RuntimeError(f"Unsupported architecture: {arch}")
    else:
        raise RuntimeError(f"Unsupported platform: {system}")
    
    return f"{lib_name}-{target_triple}.{ext}"

def _find_or_download_library() -> Path:
    """Find or download library following Go's findOrDownloadLibrary() logic exactly"""
    global _baml_shared_library_path
    
    # 1. Check explicit path set via set_shared_library_path()
    if _baml_shared_library_path:
        path = Path(_baml_shared_library_path)
        if path.exists():
            logger.debug(f"Using BAML library path set via set_shared_library_path(): {path}")
            return path
        raise RuntimeError(f"Path explicitly set via set_shared_library_path() {path} is invalid")
    
    # 2. Check environment variable BAML_LIBRARY_PATH
    env_path = os.environ.get(BAML_LIBRARY_PATH_ENV)
    if env_path:
        path = Path(env_path)
        if path.exists():
            logger.debug(f"Using BAML library path from environment variable {BAML_LIBRARY_PATH_ENV}: {path}")
            _baml_shared_library_path = str(path)
            return path
        raise RuntimeError(f"Path from environment variable {BAML_LIBRARY_PATH_ENV} ({env_path}) is invalid")
    
    # 3. Check cache directory
    cache_dir = _get_cache_dir()
    lib_filename = _get_target_lib_filename()
    cached_lib_path = cache_dir / lib_filename
    logger.debug(f"Checking for cached BAML library: {cached_lib_path}")
    
    if cached_lib_path.exists():
        logger.info(f"Found valid cached BAML library: {cached_lib_path}")
        _baml_shared_library_path = str(cached_lib_path)
        return cached_lib_path
    
    logger.debug(f"Library not found in cache: {cached_lib_path}")
    
    # 4. Try to download (if not disabled)
    if os.environ.get(BAML_DISABLE_DL_ENV, "").lower() == "true":
        logger.warning(f"Automatic download disabled via environment variable {BAML_DISABLE_DL_ENV}")
    else:
        logger.debug(f"Attempting to download BAML library version v{VERSION} for {platform.system()}/{platform.machine()}")
        # TODO: Implement download_baml_library() in next phase
        # For now, we'll skip download and continue to system paths
        pass
    
    # 5. Check default system library paths
    logger.debug("Checking default system library paths")
    checked_paths = []
    
    if platform.system() == "Darwin":
        paths = [f"/usr/local/lib/libbaml-{VERSION}.dylib", "/usr/local/lib/libbaml.dylib"]
    elif platform.system() == "Linux":
        paths = [f"/usr/local/lib/libbaml-{VERSION}.so", "/usr/local/lib/libbaml.so"]
    else:
        paths = []
    
    checked_paths = paths
    for p in paths:
        path = Path(p)
        if path.exists():
            logger.warning(f"Found BAML library in a default system path. This might lead to version/architecture mismatches. "
                         f"Path: {path}. Consider using cache or {BAML_LIBRARY_PATH_ENV} env var")
            _baml_shared_library_path = str(path)
            return path
    
    # Build detailed error message matching Go
    error_msg = f"Could not find BAML library v{VERSION} for {platform.system()}/{platform.machine()}.\n"
    error_msg += "       Resolution attempts failed:\n"
    error_msg += f"       - Explicit path (set_shared_library_path): Not set\n"
    error_msg += f"       - Environment var ({BAML_LIBRARY_PATH_ENV}): {env_path or 'Not set'}\n"
    error_msg += f"       - Cache path: {cached_lib_path} (not found)\n"
    error_msg += f"       - Download ({BAML_DISABLE_DL_ENV}): "
    if os.environ.get(BAML_DISABLE_DL_ENV, "").lower() == "true":
        error_msg += "Disabled\n"
    else:
        error_msg += "Attempted but failed\n"
    error_msg += f"       - Default system paths: {checked_paths} (not found)"
    
    raise RuntimeError(error_msg)

def load_library() -> ctypes.CDLL:
    """Load the BAML CFFI library"""
    lib_path = _find_or_download_library()
    return ctypes.CDLL(str(lib_path))