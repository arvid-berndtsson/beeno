#!/usr/bin/env python3
import argparse
import pathlib
import re
import sys

SEMVER_RE = re.compile(r"^v?(\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?)$")


def normalize_version(raw: str) -> str:
    match = SEMVER_RE.match(raw.strip())
    if not match:
        raise ValueError(
            f"Invalid version '{raw}'. Expected vMAJOR.MINOR.PATCH (optional pre-release/build metadata)."
        )
    return match.group(1)


def replace_workspace_version(content: str, version: str) -> str:
    section_re = re.compile(r"(\[workspace\.package\]\n)(.*?)(\n\[|\Z)", re.DOTALL)
    section_match = section_re.search(content)
    if not section_match:
        raise ValueError("Could not find [workspace.package] section in Cargo.toml")

    body = section_match.group(2)
    new_body, count = re.subn(
        r'(?m)^version\s*=\s*"[^"]+"\s*$',
        f'version = "{version}"',
        body,
        count=1,
    )
    if count != 1:
        raise ValueError("Could not update workspace.package version in Cargo.toml")

    return content[: section_match.start(2)] + new_body + content[section_match.end(2) :]


def replace_cli_core_dep_version(content: str, version: str) -> str:
    dep_re = re.compile(
        r'(?m)^(beeno_core\s*=\s*\{\s*path\s*=\s*"\.\./core",\s*version\s*=\s*")([^"]+)("\s*\})$'
    )
    new_content, count = dep_re.subn(rf'\g<1>{version}\3', content, count=1)
    if count != 1:
        raise ValueError("Could not update beeno_core dependency version in crates/cli/Cargo.toml")
    return new_content


def main() -> int:
    parser = argparse.ArgumentParser(description="Sync Cargo versions from release tag")
    parser.add_argument("--version", required=True, help="Release version, e.g. v0.2.0 or 0.2.0")
    parser.add_argument("--repo-root", default=".", help="Repository root path")
    args = parser.parse_args()

    version = normalize_version(args.version)
    root = pathlib.Path(args.repo_root).resolve()

    root_cargo = root / "Cargo.toml"
    cli_cargo = root / "crates" / "cli" / "Cargo.toml"

    root_content = root_cargo.read_text(encoding="utf-8")
    cli_content = cli_cargo.read_text(encoding="utf-8")

    new_root = replace_workspace_version(root_content, version)
    new_cli = replace_cli_core_dep_version(cli_content, version)

    root_cargo.write_text(new_root, encoding="utf-8")
    cli_cargo.write_text(new_cli, encoding="utf-8")

    print(f"Synchronized Cargo versions to {version}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except ValueError as exc:
        print(f"error: {exc}", file=sys.stderr)
        raise SystemExit(2)
