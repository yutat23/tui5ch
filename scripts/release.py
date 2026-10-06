"""Validate releases and package only explicitly allowed public files (Python 3.11+)."""

import argparse
import hashlib
from pathlib import Path
import re
import subprocess
import tarfile
import tomllib
import zipfile

ROOT = Path(__file__).resolve().parent.parent
DOCUMENTS = [
    "README.md", "LICENSE", "CONTRIBUTING.md", "CHANGELOG.md",
    "docs/usage.md", "docs/configuration.md", "docs/demo-screen.txt",
]


def version():
    return tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))["package"]["version"]


def release_notes(tag):
    if not re.fullmatch(r"v\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?", tag):
        raise ValueError("Release tag must be vMAJOR.MINOR.PATCH (optional prerelease suffix)")
    if tag != "v" + version():
        raise ValueError("Release tag does not match Cargo.toml version")
    changelog = (ROOT / "CHANGELOG.md").read_text(encoding="utf-8")
    match = re.search(
        rf"^## {re.escape(tag[1:])} — \d{{4}}-\d{{2}}-\d{{2}}\s*\n(.*?)(?=^## |\Z)",
        changelog, re.MULTILINE | re.DOTALL,
    )
    if not match or not match[1].strip():
        raise ValueError("CHANGELOG.md must contain a dated release section with notes")
    return match[1].strip() + "\n"


def package(output):
    host = subprocess.check_output(["rustc", "-vV"], text=True)
    target = next(line.removeprefix("host: ") for line in host.splitlines() if line.startswith("host: "))
    windows = "-windows-" in target
    binary = "tui5ch.exe" if windows else "tui5ch"
    files = [(ROOT / "target" / "release" / binary, binary)]
    files += [(ROOT / doc, doc) for doc in DOCUMENTS]
    for source, _ in files:
        if not source.is_file():
            raise FileNotFoundError(source)
    output.mkdir(parents=True, exist_ok=True)
    archive = output / f"tui5ch-{version()}-{target}{'.zip' if windows else '.tar.gz'}"
    if windows:
        with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as bundle:
            for source, name in files:
                bundle.write(source, name)
    else:
        with tarfile.open(archive, "w:gz") as bundle:
            for source, name in files:
                info = bundle.gettarinfo(source, arcname=name)
                if name == binary:
                    info.mode = 0o755
                with source.open("rb") as data:
                    bundle.addfile(info, data)
    print(archive)


def checksums(directory):
    files = sorted(p for p in directory.iterdir() if p.name.endswith((".zip", ".tar.gz")))
    if not files:
        raise ValueError("No release archives found")
    content = "".join(f"{hashlib.sha256(p.read_bytes()).hexdigest()}  {p.name}\n" for p in files)
    (directory / "SHA256SUMS").write_text(content, encoding="utf-8", newline="\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=["validate", "package", "checksums"])
    parser.add_argument("--tag")
    parser.add_argument("--output", type=Path, default=ROOT / "dist")
    args = parser.parse_args()
    if args.command == "validate":
        if not args.tag:
            parser.error("validate requires --tag")
        args.output.mkdir(parents=True, exist_ok=True)
        (args.output / "release-notes.md").write_text(release_notes(args.tag), encoding="utf-8", newline="\n")
    elif args.command == "package":
        package(args.output)
    else:
        checksums(args.output)


if __name__ == "__main__":
    main()
