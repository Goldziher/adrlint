"""Locate the adrlint executable, downloading and verifying a release asset when needed.

Environment
-----------
``ADRLINT_BINARY``
    Absolute path to an already-installed adrlint executable. When set it is used as-is: nothing is
    downloaded and no checksum is checked, so the caller owns the integrity of that file.

The downloaded binary is cached under ``~/.cache/adrlint/<version>/``. The directory is private
(mode 0o700) and sits next to ``<binary>.sha256``, which records the digest of the extracted binary
and of the archive it came from. A cache hit is only served after both digests are re-checked
against the file on disk and against the release checksum manifest; otherwise the binary is
discarded and downloaded again. Archive checksums are always verified before extraction.
"""

from __future__ import annotations

import hashlib
import os
import platform
import shutil
import ssl
import subprocess
import sys
import tarfile
import tempfile
import zipfile
from pathlib import Path
from urllib.error import URLError
from urllib.request import Request, urlopen

import certifi

BASE_URL = "https://github.com/Goldziher/adrlint/releases/download"

# A release archive is a few megabytes. Refusing anything larger keeps a hostile or broken
# response from being buffered into memory.
MAX_DOWNLOAD_BYTES = 100 * 1024 * 1024


def _is_musl() -> bool:
    """Whether this Linux runtime uses musl rather than glibc."""
    if platform.system().lower() != "linux":
        return False
    try:
        # glibc answers this; musl does not implement the GNU libc version string.
        return not os.confstr("CS_GNU_LIBC_VERSION")
    except (AttributeError, OSError, ValueError):
        return True


def _platform_triple() -> str:
    system = platform.system().lower()
    machine = platform.machine().lower()

    if system == "windows":
        if machine in {"amd64", "x86_64"}:
            return "x86_64-pc-windows-msvc"
        if machine in {"arm64", "aarch64"}:
            return "aarch64-pc-windows-msvc"
        if machine in {"x86", "i386", "i686"}:
            raise RuntimeError("32-bit Windows is not supported")
    elif system == "linux":
        libc = "musl" if _is_musl() else "gnu"
        if machine in {"amd64", "x86_64"}:
            return f"x86_64-unknown-linux-{libc}"
        if machine in {"aarch64", "arm64"}:
            return f"aarch64-unknown-linux-{libc}"
    elif system == "darwin":
        if machine in {"amd64", "x86_64"}:
            return "x86_64-apple-darwin"
        if machine in {"aarch64", "arm64"}:
            return "aarch64-apple-darwin"

    raise RuntimeError(f"Unsupported platform: {system} {machine}")


def _python_version_to_tag(version: str) -> str:
    if "rc" in version:
        core, suffix = version.split("rc")
        return f"{core}-rc.{suffix}"
    return version


def _asset(version: str) -> tuple[str, str, str]:
    tag = _python_version_to_tag(version)
    triple = _platform_triple()
    ext = "zip" if "windows" in triple else "tar.gz"
    name = f"adrlint-{triple}.{ext}"
    return f"{BASE_URL}/v{tag}/{name}", name, ext


def _get(url: str) -> bytes:
    request = Request(url, headers={"User-Agent": "adrlint-python-wrapper"})
    context = ssl.create_default_context(cafile=certifi.where())
    try:
        with urlopen(request, timeout=30, context=context) as response:
            if response.status != 200:
                raise RuntimeError(f"HTTP {response.status}: {response.reason}")
            declared = response.headers.get("Content-Length")
            if declared is not None and int(declared) > MAX_DOWNLOAD_BYTES:
                raise RuntimeError(f"Refusing to download {url}: {declared} bytes is too large")

            # Read in bounded chunks so a chunked response cannot buffer past the cap either.
            body = bytearray()
            while chunk := response.read(1024 * 1024):
                body.extend(chunk)
                if len(body) > MAX_DOWNLOAD_BYTES:
                    raise RuntimeError(f"Refusing to download {url}: response exceeds {MAX_DOWNLOAD_BYTES} bytes")
            return bytes(body)
    except URLError as exc:
        raise RuntimeError(f"Failed to download {url}: {exc}") from exc


def _digest_for(manifest: str, name: str) -> str | None:
    for line in manifest.splitlines():
        parts = line.split()
        if len(parts) == 2 and parts[1].lstrip("*") == name:
            return parts[0]
    return None


def _expected_digest(version: str, asset_name: str) -> str:
    tag = _python_version_to_tag(version)
    manifest_url = f"{BASE_URL}/v{tag}/adrlint_{tag}_checksums.txt"
    digest = _digest_for(_get(manifest_url).decode("utf-8"), asset_name)
    if digest is None:
        raise RuntimeError(f"No checksum published for {asset_name}")
    return digest


def _download(version: str) -> tuple[bytes, str, str, str]:
    url, asset_name, ext = _asset(version)
    archive = _get(url)
    expected = _expected_digest(version, asset_name)
    actual = hashlib.sha256(archive).hexdigest()
    if actual != expected:
        raise RuntimeError(f"Checksum mismatch for {asset_name}: expected {expected}, got {actual}")
    return archive, asset_name, ext, expected


def _extract(archive: Path, ext: str, destination: Path) -> None:
    if ext == "zip":
        with zipfile.ZipFile(archive) as zf:
            for name in zf.namelist():
                if name.endswith("adrlint") or name.endswith("adrlint.exe"):
                    with zf.open(name) as src, destination.open("wb") as dst:
                        dst.write(src.read())
                    return
    else:
        with tarfile.open(archive, "r:gz") as tar:
            for member in tar.getmembers():
                if member.name.endswith("adrlint") or member.name.endswith("adrlint.exe"):
                    with tar.extractfile(member) as src, destination.open("wb") as dst:
                        dst.write(src.read())
                    return
    raise RuntimeError("Binary not found in downloaded archive")


def _cache_dir(version: str) -> Path:
    cache_dir = Path.home() / ".cache" / "adrlint" / version
    cache_dir.mkdir(parents=True, exist_ok=True, mode=0o700)
    # mkdir's mode is filtered by the umask and is ignored when the directory already exists.
    try:
        os.chmod(cache_dir, 0o700)
    except OSError:
        pass
    return cache_dir


def _sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _write_cache_file(destination: Path, source: Path, mode: int) -> None:
    """Copy ``source`` into ``destination`` without ever writing through an existing path.

    The content is written to a fresh ``O_EXCL`` file in the destination directory and then
    renamed into place, so a symlink left at ``destination`` is replaced rather than followed.
    """
    fd, staging = tempfile.mkstemp(dir=str(destination.parent), prefix=f".{destination.name}.")
    try:
        with os.fdopen(fd, "wb") as target, source.open("rb") as origin:
            shutil.copyfileobj(origin, target)
            target.flush()
            os.fsync(target.fileno())
        os.chmod(staging, mode)
        os.replace(staging, destination)
    except BaseException:
        Path(staging).unlink(missing_ok=True)
        raise


def _cache_is_valid(binary_path: Path, record_path: Path, version: str, asset_name: str) -> bool:
    """Check a cached binary against the record written at install time and the live manifest."""
    if not binary_path.is_file() or not record_path.is_file():
        return False

    published_archive = _expected_digest(version, asset_name)
    record = record_path.read_text(encoding="utf-8")
    if _digest_for(record, asset_name) != published_archive:
        return False

    recorded_binary = _digest_for(record, binary_path.name)
    if recorded_binary is None:
        return False

    return _sha256_file(binary_path) == recorded_binary


def ensure_binary() -> str:
    """Ensure the binary is available, downloading if necessary."""
    from . import __version__

    override = os.getenv("ADRLINT_BINARY")
    if override:
        return override

    suffix = ".exe" if platform.system().lower() == "windows" else ""
    cache_dir = _cache_dir(__version__)
    binary_path = cache_dir / f"adrlint{suffix}"
    record_path = cache_dir / f"adrlint{suffix}.sha256"
    _, asset_name, _ = _asset(__version__)

    if binary_path.is_file() and os.access(binary_path, os.X_OK):
        if _cache_is_valid(binary_path, record_path, __version__, asset_name):
            return str(binary_path)
        print("Cached adrlint binary failed verification; reinstalling.", file=sys.stderr)

    binary_path.unlink(missing_ok=True)
    record_path.unlink(missing_ok=True)

    print(f"Downloading adrlint binary v{__version__}...", file=sys.stderr)
    archive, asset_name, ext, archive_digest = _download(__version__)

    with tempfile.TemporaryDirectory() as tmpdir:
        archive_path = Path(tmpdir) / asset_name
        archive_path.write_bytes(archive)
        extracted = Path(tmpdir) / binary_path.name
        _extract(archive_path, ext, extracted)

        _write_cache_file(binary_path, extracted, 0o755)
        record = f"{_sha256_file(binary_path)}  {binary_path.name}\n{archive_digest}  {asset_name}\n"
        record_file = Path(tmpdir) / record_path.name
        record_file.write_text(record, encoding="utf-8")
        _write_cache_file(record_path, record_file, 0o600)

    print("Binary downloaded successfully!", file=sys.stderr)
    return str(binary_path)


def run_adrlint(args: list[str]) -> None:
    """Run the adrlint binary with the given arguments."""
    binary_path = ensure_binary()

    try:
        result = subprocess.run([binary_path] + args, check=False)
        sys.exit(result.returncode)
    except FileNotFoundError:
        raise RuntimeError(f"Binary not found at {binary_path}")
    except Exception as exc:
        raise RuntimeError(f"Failed to run adrlint: {exc}")
