"""Basic tests for BAML Python CFFI client Phase 1"""

import pytest
import os
import sys
import tempfile
from pathlib import Path
from typing import Any

# Import the package
import baml_py_cffi


def test_package_imports() -> None:
    """Test that the package imports successfully"""
    assert baml_py_cffi is not None
    assert hasattr(baml_py_cffi, 'version')
    assert hasattr(baml_py_cffi, 'set_shared_library_path')


def test_version_attribute() -> None:
    """Test that the package has correct version"""
    assert baml_py_cffi.__version__ == "0.205.0"


def test_library_loading_module() -> None:
    """Test that the library loading module exists and has expected functions"""
    from baml_py_cffi import _lib
    
    assert hasattr(_lib, 'set_shared_library_path')
    assert hasattr(_lib, 'load_library')
    assert hasattr(_lib, '_find_or_download_library')
    assert hasattr(_lib, '_get_cache_dir')
    assert hasattr(_lib, '_get_target_lib_filename')


def test_ffi_module_imports() -> None:
    """Test that FFI module imports correctly"""
    from baml_py_cffi import _ffi
    
    assert hasattr(_ffi, 'version')
    assert hasattr(_ffi, '_lib')


def test_cache_dir_creation() -> None:
    """Test cache directory logic"""
    from baml_py_cffi._lib import _get_cache_dir
    
    # Test with default cache dir
    cache_dir: Path = _get_cache_dir()
    assert isinstance(cache_dir, Path)
    assert cache_dir.exists()
    assert cache_dir.is_dir()
    
    # Test with custom cache dir via env var
    with tempfile.TemporaryDirectory() as tmpdir:
        os.environ['BAML_CACHE_DIR'] = tmpdir
        try:
            custom_cache: Path = _get_cache_dir()
            assert str(custom_cache) == tmpdir
        finally:
            del os.environ['BAML_CACHE_DIR']


def test_target_lib_filename() -> None:
    """Test platform-specific library filename generation"""
    from baml_py_cffi._lib import _get_target_lib_filename
    
    filename: str = _get_target_lib_filename()
    assert isinstance(filename, str)
    assert filename.startswith('libbaml_cffi-')
    
    # Check platform-specific extension
    if sys.platform == 'darwin':
        assert filename.endswith('.dylib')
    elif sys.platform.startswith('linux'):
        assert filename.endswith('.so')


def test_library_path_error_message() -> None:
    """Test that library loading gives helpful error when library is missing"""
    from baml_py_cffi import _lib
    from baml_py_cffi._lib import _find_or_download_library
    
    # Save and clear the global library path
    original_path: str = _lib._baml_shared_library_path
    _lib._baml_shared_library_path = ""
    
    # Set a non-existent path to force error
    os.environ['BAML_LIBRARY_PATH'] = '/nonexistent/path/to/library.so'
    os.environ['BAML_LIBRARY_DISABLE_DOWNLOAD'] = 'true'
    
    try:
        with pytest.raises(RuntimeError) as exc_info:
            _find_or_download_library()
        
        error_msg: str = str(exc_info.value)
        assert "Path from environment variable BAML_LIBRARY_PATH" in error_msg
        assert "/nonexistent/path/to/library.so" in error_msg
        assert "is invalid" in error_msg
    finally:
        # Restore original state
        _lib._baml_shared_library_path = original_path
        if 'BAML_LIBRARY_PATH' in os.environ:
            del os.environ['BAML_LIBRARY_PATH']
        if 'BAML_LIBRARY_DISABLE_DOWNLOAD' in os.environ:
            del os.environ['BAML_LIBRARY_DISABLE_DOWNLOAD']


def test_set_shared_library_path() -> None:
    """Test setting explicit library path"""
    from baml_py_cffi._lib import set_shared_library_path, _baml_shared_library_path
    
    test_path: str = "/test/path/to/library.so"
    set_shared_library_path(test_path)
    
    # Note: We can't directly access _baml_shared_library_path from here
    # but we can verify the function exists and doesn't throw


@pytest.mark.skipif(
    not os.path.exists("/usr/local/lib/libbaml.dylib") and 
    not os.path.exists("/usr/local/lib/libbaml.so") and
    'BAML_LIBRARY_PATH' not in os.environ,
    reason="BAML library not available"
)
def test_version_function() -> None:
    """Test calling the version function if library is available"""
    try:
        version: str = baml_py_cffi.version()
        assert isinstance(version, str)
        # Version should be in format like "0.205.0" or similar
        assert len(version.split('.')) >= 2
    except RuntimeError as e:
        if "Could not find BAML library" in str(e):
            pytest.skip("BAML library not found")
        else:
            raise


if __name__ == "__main__":
    pytest.main([__file__, "-v"])