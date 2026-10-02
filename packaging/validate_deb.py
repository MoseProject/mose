#!/usr/bin/env python3
"""Validate that a Debian package installs the complete offline oca bundle."""

import argparse
import pathlib
import subprocess
import sys
import tempfile


def dpkg_field(package: pathlib.Path, field: str) -> str:
    return subprocess.check_output(
        ["dpkg-deb", "--field", package, field], text=True
    ).strip()


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("package", type=pathlib.Path)
    parser.add_argument("--version", required=True)
    parser.add_argument(
        "--manifest",
        type=pathlib.Path,
        default=pathlib.Path(__file__).with_name("bundle-manifest.json"),
    )
    args = parser.parse_args()
    package = args.package.resolve()
    script_dir = pathlib.Path(__file__).resolve().parent

    expected_fields = {
        "Package": "oca",
        "Version": args.version,
        "Architecture": "amd64",
        "Depends": "libc6 (>= 2.28)",
    }
    errors = [
        f"{field}: expected {expected!r}, got {actual!r}"
        for field, expected in expected_fields.items()
        if (actual := dpkg_field(package, field)) != expected
    ]

    contents = subprocess.check_output(
        ["dpkg-deb", "--contents", package], text=True
    )
    non_root_entries = [
        line for line in contents.splitlines() if " root/root " not in line
    ]
    if non_root_entries:
        errors.append("package contains entries not owned by root:root")

    with tempfile.TemporaryDirectory() as temp:
        root = pathlib.Path(temp)
        subprocess.run(["dpkg-deb", "--extract", package, root], check=True)
        subprocess.run(
            [
                sys.executable,
                script_dir / "validate_bundle.py",
                root / "opt" / "oca",
                "--platform",
                "linux",
                "--manifest",
                args.manifest,
            ],
            check=True,
        )

        command = root / "usr" / "bin" / "oca"
        if not command.is_symlink() or command.readlink() != pathlib.Path("/opt/oca/oca-desktop"):
            errors.append("usr/bin/oca is not a symlink to /opt/oca/oca-desktop")

        desktop_path = root / "usr" / "share" / "applications" / "oca.desktop"
        desktop = desktop_path.read_text(encoding="utf-8") if desktop_path.is_file() else ""
        for entry in ("TryExec=oca", "Exec=oca", "Icon=oca"):
            if entry not in desktop.splitlines():
                errors.append(f"desktop entry is missing {entry}")

        for relative in (
            "usr/share/icons/hicolor/1024x1024/apps/oca.png",
            "usr/share/doc/oca/copyright",
        ):
            if not (root / relative).is_file():
                errors.append(relative)

    with tempfile.TemporaryDirectory() as temp:
        control = pathlib.Path(temp)
        subprocess.run(["dpkg-deb", "--control", package, control], check=True)
        for script in ("postinst", "postrm"):
            path = control / script
            if not path.is_file() or path.stat().st_mode & 0o111 == 0:
                errors.append(f"executable DEBIAN/{script}")
        if not (control / "md5sums").is_file():
            errors.append("DEBIAN/md5sums")

    if errors:
        print("Debian package validation failed:", file=sys.stderr)
        for error in errors:
            print(f"  - {error}", file=sys.stderr)
        raise SystemExit(1)
    print(f"validated offline Debian package at {package}")


if __name__ == "__main__":
    main()
