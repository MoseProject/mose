#!/usr/bin/env python3
"""Build and validate a small Debian package fixture without external downloads."""

import hashlib
import json
import pathlib
import shutil
import subprocess
import sys
import tempfile


def write(path: pathlib.Path, data: bytes = b"fixture") -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)


def sha256(path: pathlib.Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main() -> None:
    if shutil.which("dpkg-deb") is None:
        print("skipped Debian package fixture: dpkg-deb is unavailable")
        return

    repo = pathlib.Path(__file__).resolve().parents[2]
    with tempfile.TemporaryDirectory() as temp:
        root = pathlib.Path(temp)
        bundle = root / "bundle"
        executable_paths = [
            "oca-desktop",
            "ytbridge",
            "deno",
            "resources/runtime/bin/ffmpeg",
        ]
        for relative in executable_paths:
            path = bundle / relative
            write(path)
            path.chmod(0o755)

        write(bundle / "espeak-ng-data" / "fixture")
        write(bundle / "resources/models/test-model.bin", b"model")
        write(bundle / "resources/licenses/oca.txt")
        write(
            bundle / "resources/licenses/THIRD_PARTY_NOTICES.txt",
            b"Component: fixture\nComponent: FFmpeg\nComponent: CPython\n"
            b"Component: yt-dlp\nComponent: yt-dlp EJS scripts\nComponent: Deno\n",
        )
        write(bundle / "resources/licenses/fonts/test-OFL.txt")
        write(bundle / "resources/runtime/lib/libavcodec.so.1")
        write(bundle / "resources/runtime/gstreamer/lib/libgstreamer-1.0.so.0")
        write(bundle / "resources/runtime/gstreamer/lib/gstreamer-1.0/plugin.so")
        write(bundle / "lib/libpython3.10.so.1.0")
        write(bundle / "lib/python3.10/site-packages/yt_dlp/__init__.py")
        write(bundle / "lib/python3.10/site-packages/yt_dlp_ejs/__init__.py")

        versions = {
            "FFmpeg": "test-ffmpeg",
            "CPython": "3.10.21+fixture",
            "yt-dlp": "test-ytdlp",
            "yt-dlp EJS scripts": "test-ejs",
            "Deno": "test-deno",
        }
        manifest = {
            "schema_version": 1,
            "models": [
                {
                    "name": "fixture",
                    "path": "models/test-model.bin",
                    "sha256": sha256(bundle / "resources/models/test-model.bin"),
                }
            ],
            "native_dependencies": [
                {"name": name, "version": version, "bundle": "runtime", "license": "MIT"}
                for name, version in versions.items()
            ],
            "system_contract": {},
        }
        manifest_path = root / "manifest.json"
        manifest_path.write_text(json.dumps(manifest), encoding="utf-8")
        (bundle / "resources/bundle-manifest.json").write_text(
            json.dumps(manifest), encoding="utf-8"
        )
        (bundle / "resources/RUNTIME_VERSIONS.txt").write_text(
            "\n".join(versions.values()) + "\n", encoding="utf-8"
        )

        inventory_path = bundle / "resources/DEPENDENCIES.json"
        inventory = {
            "bundled_files": [
                {
                    "path": path.relative_to(bundle).as_posix(),
                    "size": path.stat().st_size,
                    "sha256": sha256(path),
                }
                for path in sorted(bundle.rglob("*"))
                if path.is_file() and path != inventory_path
            ]
        }
        inventory_path.write_text(json.dumps(inventory), encoding="utf-8")

        package = root / "oca_1.2.3_amd64.deb"
        subprocess.run(
            [repo / "packaging/build_deb.sh", bundle, "1.2.3", package, manifest_path],
            check=True,
        )
        subprocess.run(
            [
                sys.executable,
                repo / "packaging/validate_deb.py",
                package,
                "--version",
                "1.2.3",
                "--manifest",
                manifest_path,
            ],
            check=True,
        )


if __name__ == "__main__":
    main()
