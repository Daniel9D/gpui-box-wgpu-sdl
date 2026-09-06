#!/usr/bin/env python3
"""Reject changes outside the documented GPUI Box fork patch allowlist."""

from __future__ import annotations

import hashlib
import io
import subprocess
import sys
import tarfile
import urllib.error
import urllib.request
from pathlib import Path
from typing import BinaryIO


REPOSITORY_ROOT = Path(__file__).resolve().parents[1]
VENDOR_ROOT = REPOSITORY_ROOT / "vendor" / "gpui-box"
MANIFEST = VENDOR_ROOT / "FORK_PATCHES.md"


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def relative_files(root: Path) -> dict[str, str]:
    return {
        path.relative_to(root).as_posix(): sha256(path)
        for path in root.rglob("*")
        if path.is_file()
    }


def unexpected_drift(
    upstream_root: Path, vendor_root: Path, allowed: set[str]
) -> list[str]:
    upstream = relative_files(upstream_root)
    vendor = relative_files(vendor_root)
    return sorted(
        path
        for path in upstream.keys() | vendor.keys()
        if path not in allowed and upstream.get(path) != vendor.get(path)
    )


def archive_files(source: BinaryIO) -> dict[str, bytes]:
    files: dict[str, bytes] = {}
    with tarfile.open(fileobj=source, mode="r:*") as archive:
        for member in archive:
            parts = Path(member.name).parts
            if len(parts) < 2:
                continue
            relative = Path(*parts[1:]).as_posix()
            if member.isfile():
                extracted = archive.extractfile(member)
                if extracted is None:
                    raise ValueError(f"cannot read archive member {member.name}")
                files[relative] = extracted.read()
            elif member.issym():
                files[relative] = member.linkname.encode("utf-8")
    return files


def manifest_configuration(path: Path) -> tuple[str, set[str]]:
    revision = None
    allowed: set[str] = set()
    in_allowlist = False
    for line in path.read_text(encoding="utf-8").splitlines():
        if line.startswith("Revision: "):
            revision = line.removeprefix("Revision: ").strip()
        elif line == "## Allowed differences":
            in_allowlist = True
        elif in_allowlist and line.startswith("- "):
            allowed.add(line.removeprefix("- ").strip())
    if not revision or len(revision) != 40:
        raise ValueError(f"missing exact 40-character Revision in {path}")
    return revision, allowed


def upstream_archive(revision: str) -> io.BytesIO:
    url = f"https://github.com/fran0220/gpui-box/archive/{revision}.tar.gz"
    try:
        with urllib.request.urlopen(url, timeout=60) as response:
            return io.BytesIO(response.read())
    except (OSError, urllib.error.URLError) as download_error:
        local = subprocess.run(
            [
                "git",
                "archive",
                "--format=tar.gz",
                f"--prefix=gpui-box-{revision}/",
                revision,
            ],
            cwd=REPOSITORY_ROOT,
            capture_output=True,
            check=False,
        )
        if local.returncode == 0:
            print(
                f"warning: GitHub archive unavailable; using local Git object: {download_error}",
                file=sys.stderr,
            )
            return io.BytesIO(local.stdout)
        raise RuntimeError(
            f"cannot obtain upstream revision {revision}: {download_error}"
        ) from download_error


def compare_archive(
    upstream: dict[str, bytes], vendor_root: Path, allowed: set[str]
) -> list[str]:
    vendor = {
        path.relative_to(vendor_root).as_posix(): path.read_bytes()
        for path in vendor_root.rglob("*")
        if path.is_file()
    }
    return sorted(
        path
        for path in upstream.keys() | vendor.keys()
        if path not in allowed and upstream.get(path) != vendor.get(path)
    )


def main() -> int:
    revision, allowed = manifest_configuration(MANIFEST)
    drift = compare_archive(archive_files(upstream_archive(revision)), VENDOR_ROOT, allowed)
    if drift:
        print("Unexpected GPUI Box vendor drift:", file=sys.stderr)
        for path in drift:
            print(f"- {path}", file=sys.stderr)
        return 1
    print(f"GPUI Box vendor matches {revision} outside {len(allowed)} allowed paths.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
