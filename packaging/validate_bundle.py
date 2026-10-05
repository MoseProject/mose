#!/usr/bin/env python3
"""Reject a release tree that would need a runtime download or system multimedia SDK."""

import argparse
import hashlib
import json
import pathlib
import sys


def any_file(root: pathlib.Path, patterns: tuple[str, ...]) -> bool:
    return any(path.is_file() for pattern in patterns for path in root.glob(pattern))


def sha256(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("bundle", type=pathlib.Path)
    parser.add_argument("--platform", required=True, choices=("windows", "linux", "macos"))
    parser.add_argument("--inventory", type=pathlib.Path)
    parser.add_argument(
        "--manifest",
        type=pathlib.Path,
        default=pathlib.Path(__file__).with_name("bundle-manifest.json"),
    )
    args = parser.parse_args()
    root = args.bundle.resolve()
    manifest = json.loads(args.manifest.read_text(encoding="utf-8"))
    missing: list[str] = []

    if args.platform == "macos":
        executable_root = root / "Contents" / "MacOS"
        resources = root / "Contents" / "Resources"
        executable = "oca"
        helper = "ytbridge"
        deno = "deno"
    else:
        executable_root = root
        resources = root / "resources"
        executable = "oca-desktop.exe" if args.platform == "windows" else "oca-desktop"
        helper = "ytbridge.exe" if args.platform == "windows" else "ytbridge"
        deno = "deno.exe" if args.platform == "windows" else "deno"
    for relative in (executable, helper, deno):
        if not (executable_root / relative).is_file():
            missing.append(str((executable_root / relative).relative_to(root)))
    for model in manifest["models"]:
        model_path = resources / model["path"]
        if not model_path.is_file():
            missing.append(str(model_path.relative_to(root)))
        elif sha256(model_path) != model["sha256"]:
            missing.append(f"{model_path.relative_to(root)} (SHA-256 mismatch)")
    for relative in (
        "RUNTIME_VERSIONS.txt",
        "bundle-manifest.json",
        "licenses/oca.txt",
        "licenses/THIRD_PARTY_NOTICES.txt",
    ):
        if not (resources / relative).is_file():
            missing.append(str((resources / relative).relative_to(root)))
    notices_path = resources / "licenses" / "THIRD_PARTY_NOTICES.txt"
    if notices_path.is_file():
        notices = notices_path.read_text(encoding="utf-8")
        for component in manifest["models"] + [
            item
            for item in manifest["native_dependencies"]
            if "build-time only" not in item["bundle"]
        ]:
            if f'Component: {component["name"]}' not in notices:
                missing.append(f'THIRD_PARTY_NOTICES.txt entry for {component["name"]}')
    if not any_file(resources / "licenses" / "fonts", ("*-OFL.txt",)):
        missing.append(str((resources / "licenses" / "fonts").relative_to(root)) + "/*-OFL.txt")

    runtime = resources / "runtime"
    ffmpeg = "ffmpeg.exe" if args.platform == "windows" else "ffmpeg"
    if not (runtime / "bin" / ffmpeg).is_file():
        missing.append(f"resources/runtime/bin/{ffmpeg}")
    if args.platform == "windows":
        if not any_file(root, ("avcodec-*.dll", "libavcodec-*.dll")):
            missing.append("FFmpeg avcodec DLL")
        if not (root / "python310.dll").is_file() or not (root / "Lib").is_dir():
            missing.append("private CPython runtime")
        site_packages = root / "Lib" / "site-packages"
    elif args.platform == "linux":
        if not any_file(runtime, ("lib/libavcodec.so*",)):
            missing.append("resources/runtime/lib/libavcodec.so*")
        if not any_file(root, ("lib/libpython3.10.so*",)):
            missing.append("private CPython runtime")
        site_packages = root / "lib" / "python3.10" / "site-packages"
    else:
        frameworks = root / "Contents" / "Frameworks"
        if not any_file(frameworks, ("libavcodec*.dylib",)):
            missing.append("Contents/Frameworks/libavcodec*.dylib")
        site_packages = resources / "python" / "lib" / "python3.10" / "site-packages"
        for relative in ("Contents/Info.plist", "Contents/Resources/oca.icns"):
            if not (root / relative).is_file():
                missing.append(relative)

    if not (site_packages / "yt_dlp").is_dir():
        missing.append("private Python site-packages/yt_dlp")
    if not (site_packages / "yt_dlp_ejs").is_dir():
        missing.append("private Python site-packages/yt_dlp_ejs")

    versions_path = resources / "RUNTIME_VERSIONS.txt"
    if versions_path.is_file():
        versions = versions_path.read_text(encoding="utf-8")
        dependencies = {item["name"]: item for item in manifest["native_dependencies"]}
        expected_versions = [
            dependencies["CPython"]["version"].split("+")[0],
            dependencies["yt-dlp"]["version"],
            dependencies["yt-dlp EJS scripts"]["version"],
            dependencies["Deno"]["version"],
        ]
        if args.platform != "macos":
            expected_versions.append(dependencies["FFmpeg"]["version"])
        elif "ffmpeg version" not in versions:
            missing.append("RUNTIME_VERSIONS.txt macOS FFmpeg entry")
        for version in expected_versions:
            if version not in versions:
                missing.append(f"RUNTIME_VERSIONS.txt entry for {version}")

    inventory_path = args.inventory or resources / "DEPENDENCIES.json"
    if not inventory_path.is_file():
        missing.append(str(inventory_path))
    if inventory_path.is_file():
        inventory = json.loads(inventory_path.read_text(encoding="utf-8"))
        expected_files = {item["path"]: item for item in inventory.get("bundled_files", [])}
        actual_files = {
            path.relative_to(root).as_posix(): path
            for path in root.rglob("*")
            if path.is_file() and path.resolve() != inventory_path.resolve()
        }
        for path in sorted(actual_files.keys() - expected_files.keys()):
            missing.append(f"DEPENDENCIES.json inventory entry for {path}")
        for path in sorted(expected_files.keys() - actual_files.keys()):
            missing.append(f"bundled file listed in inventory: {path}")
        for relative in sorted(actual_files.keys() & expected_files.keys()):
            path = actual_files[relative]
            expected = expected_files[relative]
            if path.stat().st_size != expected["size"] or sha256(path) != expected["sha256"]:
                missing.append(f"bundled file matching inventory: {relative}")

    if missing:
        print("bundle validation failed; missing:", file=sys.stderr)
        for item in missing:
            print(f"  - {item}", file=sys.stderr)
        raise SystemExit(1)
    print(f"validated complete {args.platform} bundle at {root}")


if __name__ == "__main__":
    main()
