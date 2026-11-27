#!/usr/bin/env python3
"""Report repository architecture and maintenance health.

The report is intentionally static. It measures repository shape and
discoverability without interpreting a normalized project or rewriting any
expected result.
"""

from __future__ import annotations

import argparse
import json
import os
import re
from pathlib import Path


ROOT = Path(__file__).resolve().parents[3]
REQUIRED_PATHS = (
    "AGENTS.md",
    "README.md",
    "design/README.md",
    "implementation/docs/principles.md",
    "implementation/docs/architecture-three-modules.md",
    "implementation/docs/contributing.md",
    "implementation/docs/validation.md",
    "implementation/docs/agent-maintenance.md",
    "implementation/tools/normalizer",
)

MAINTENANCE_CONTRACTS = {
    "implementation/docs/agent-maintenance.md": (
        "责任层",
        "最小证据链",
        "expected.yaml",
    ),
    "implementation/docs/contributing.md": (
        "../../design/README.md",
        "../README.md",
        "../../usage/README.md",
        "../../tests/README.md",
    ),
    "implementation/docs/architecture-three-modules.md": (
        "AI 预处理",
        "统一 IR",
        "最终 PERS",
    ),
}
CANONICAL_KERNEL_INPUT_SCHEMA = "normalizer.kernel-input/1"
CANONICAL_PERS_SCHEMA = "normalizer.pers/1"


def _lines(path: Path) -> int:
    try:
        return len(path.read_text().splitlines())
    except UnicodeDecodeError:
        return 0


def _local_dependencies(manifest: Path) -> list[str]:
    dependencies: list[str] = []
    in_dependencies = False
    for line in manifest.read_text().splitlines():
        if line.startswith("["):
            in_dependencies = line in {"[dependencies]", "[dev-dependencies]"}
            continue
        if not in_dependencies:
            continue
        match = re.match(r"^([A-Za-z0-9_-]+)\s*=\s*\{\s*path\s*=\s*\"", line)
        if match:
            dependencies.append(match.group(1))
    return sorted(set(dependencies))


def _cycle_nodes(graph: dict[str, list[str]]) -> list[str]:
    visiting: set[str] = set()
    visited: set[str] = set()
    cycle: set[str] = set()

    def visit(node: str) -> None:
        if node in visiting:
            cycle.add(node)
            return
        if node in visited:
            return
        visiting.add(node)
        for dependency in graph.get(node, []):
            visit(dependency)
            if dependency in cycle:
                cycle.add(node)
        visiting.remove(node)
        visited.add(node)

    for node in graph:
        visit(node)
    return sorted(cycle)


def _maintenance_contract(root: Path) -> dict:
    """Ensure the agent-facing documents still state the agreed boundaries."""
    violations = []
    documents = {}
    for relative, markers in MAINTENANCE_CONTRACTS.items():
        path = root / relative
        text = path.read_text(encoding="utf-8") if path.is_file() else ""
        missing = [marker for marker in markers if marker not in text]
        documents[relative] = {"present": path.is_file(), "missing_markers": missing}
        if not path.is_file():
            violations.append({"path": relative, "reason": "maintenance contract is missing"})
        elif missing:
            violations.append({"path": relative, "reason": "maintenance contract markers are missing", "missing": missing})
    return {"violations": violations, "documents": documents}


# The protocol guard covers first-party material, not dependencies, caches, or
# build products. The scanner itself necessarily contains the forbidden names.
PROTOCOL_ROOTS = (
    "AGENTS.md", "README.md", "design", "implementation/contracts",
    "implementation/docs", "implementation/tools",
    "implementation/apps", "implementation/packages/normalizer/crates",
    "tests", "usage",
)
PROTOCOL_EXCLUDED_DIRS = {
    ".audit-npm-cache", ".cache", ".git", ".venv", "__pycache__", "evidence",
    "node_modules", "target", "venv",
}
PROTOCOL_TEXT_EXTENSIONS = {
    "", ".command", ".css", ".html", ".java", ".js", ".json", ".md", ".mjs",
    ".py", ".rs", ".sh", ".sql", ".toml", ".ts", ".tsx", ".txt", ".vue",
    ".yaml", ".yml",
}
PROTOCOL_SCANNER = Path("implementation/tools/architecture/health.py")
OBSOLETE_PROTOCOL = re.compile(r"normalizer\.normalized-(pure-result|behavior-result|pure)/1(?![0-9])")
OBSOLETE_TERM = re.compile(r"normalized[_-]pure", re.IGNORECASE)


def _protocol_files(root: Path) -> list[Path]:
    paths: set[Path] = set()
    for relative in PROTOCOL_ROOTS:
        location = root / relative
        if location.is_file():
            paths.add(location)
        if not location.is_dir():
            continue
        for directory, children, filenames in os.walk(location):
            children[:] = sorted(name for name in children if name not in PROTOCOL_EXCLUDED_DIRS)
            for name in filenames:
                path = Path(directory) / name
                if path.suffix in PROTOCOL_TEXT_EXTENSIONS and not path.is_symlink():
                    paths.add(path)
    return sorted(path for path in paths if path.relative_to(root) != PROTOCOL_SCANNER)


def _protocol_usage(root: Path) -> tuple[list[Path], list[str], list[dict]]:
    active_files = _protocol_files(root)
    obsolete_active: list[str] = []
    violations: list[dict] = []
    for path in active_files:
        try:
            text = path.read_text(encoding="utf-8")
        except UnicodeDecodeError:
            continue
        relative = str(path.relative_to(root))
        schemas = {match[1] for match in OBSOLETE_PROTOCOL.finditer(text)}
        if schemas:
            obsolete_active.append(relative)
            expected = []
            if "pure" in schemas:
                expected.append(CANONICAL_KERNEL_INPUT_SCHEMA)
            if schemas - {"pure"}:
                expected.append(CANONICAL_PERS_SCHEMA)
            violations.append({
                "path": relative,
                "expected": " or ".join(expected),
                "reason": "active material still uses an obsolete protocol name",
            })
        if OBSOLETE_TERM.search(OBSOLETE_PROTOCOL.sub("", text)):
            violations.append({
                "path": relative,
                "expected": "kernel_input or kernel-input terminology",
                "reason": "active material still uses an obsolete protocol term",
            })
    return active_files, obsolete_active, violations


def _kernel_input_contract(root: Path) -> dict:
    """Keep active protocol names canonical and current."""
    active_files, obsolete_active, violations = _protocol_usage(root)

    kernel_input = root / "implementation/packages/normalizer/crates/pipeline/src/kernel_input.rs"
    kernel_text = kernel_input.read_text(encoding="utf-8") if kernel_input.is_file() else ""
    if f'KERNEL_INPUT_SCHEMA: &str = "{CANONICAL_KERNEL_INPUT_SCHEMA}"' not in kernel_text:
        violations.append({"path": str(kernel_input.relative_to(root)), "reason": "canonical input schema constant is missing"})

    adapter = root / "implementation/tools/scenarios/module-project.mjs"
    adapter_text = adapter.read_text(encoding="utf-8") if adapter.is_file() else ""
    if f"schema: '{CANONICAL_KERNEL_INPUT_SCHEMA}'" not in adapter_text:
        violations.append({"path": str(adapter.relative_to(root)), "reason": "scenario adapter does not emit canonical input schema"})

    if (
        "kernel_input.stage" not in kernel_text
        or "normalized_pure.stage" in kernel_text
        or "kernel_execution.metrics" in kernel_text
        or "kernel_execution.stage" in kernel_text
    ):
        violations.append({"path": str(kernel_input.relative_to(root)), "reason": "input stage profile key is not isolated from execution diagnostics"})
    execution = root / "implementation/packages/normalizer/crates/pipeline/src/kernel_execution.rs"
    execution_text = execution.read_text(encoding="utf-8") if execution.is_file() else ""
    if (
        "kernel_execution.metrics" not in execution_text
        or "kernel_execution.stage" not in execution_text
        or "kernel_input.metrics" in execution_text
        or "normalized_pure.metrics" in execution_text
        or "phase=kernel_execution" not in execution_text
        or "phase=kernel_input" in execution_text
    ):
        violations.append({"path": str(execution.relative_to(root)), "reason": "execution profile key or phase is not canonical"})

    src = root / "implementation/packages/normalizer/crates/pipeline/src"
    result = src / "kernel_result.rs"
    entry = src / "kernel_input/business_entry.rs"
    if not result.is_file() or f'PERS_SCHEMA: &str = "{CANONICAL_PERS_SCHEMA}"' not in result.read_text(encoding="utf-8"):
        violations.append({"path": str(result.relative_to(root)), "reason": "kernel output schema differs from the public contract"})
    if not entry.is_file() or "kernel_result::PERS_SCHEMA" not in entry.read_text(encoding="utf-8"):
        violations.append({"path": str(entry.relative_to(root)), "reason": "business projection must emit the canonical schema"})
    schema = root / "implementation/contracts/pers/schema.yaml"
    if not schema.is_file() or f"$id: {CANONICAL_PERS_SCHEMA}" not in schema.read_text(encoding="utf-8"):
        violations.append({"path": str(schema.relative_to(root)), "reason": "business output schema is not canonical"})

    kernel_doc = root / "usage/kernel-input.md"
    if not kernel_doc.is_file() or CANONICAL_KERNEL_INPUT_SCHEMA not in kernel_doc.read_text(encoding="utf-8"):
        violations.append({"path": "usage/kernel-input.md", "reason": "canonical input documentation is missing"})

    return {
        "canonical": {
            "kernel_input": CANONICAL_KERNEL_INPUT_SCHEMA,
            "pers": CANONICAL_PERS_SCHEMA,
        },
        "active_materials": len(active_files),
        "obsolete_active_paths": obsolete_active,
        "violations": violations,
    }


def collect(root: Path = ROOT) -> dict:
    engine = root / "implementation/packages/normalizer"
    crates = engine / "crates"
    rust_files = sorted(crates.rglob("*.rs"))
    rust_lines = [_lines(path) for path in rust_files]
    manifests = sorted(crates.glob("*/Cargo.toml"))
    graph = {manifest.parent.name: _local_dependencies(manifest) for manifest in manifests}
    required = {path: (root / path).exists() for path in REQUIRED_PATHS}
    large = sorted(
        {
            str(path.relative_to(root)): line_count
            for path, line_count in zip(rust_files, rust_lines)
            if line_count > 2000
        }.items(),
        key=lambda item: (-item[1], item[0]),
    )
    very_large = [item for item in large if item[1] > 3000]
    return {
        "schema": "normalizer.architecture-health/3",
        "required_paths": required,
        "required_paths_missing": sorted(path for path, present in required.items() if not present),
        "rust": {
            "files": len(rust_files),
            "lines": sum(rust_lines),
            "files_over_2000_lines": large,
            "files_over_3000_lines": very_large,
        },
        "crate_graph": graph,
        "crate_cycles": _cycle_nodes(graph),
        "agent_entrypoints": {
            "root_readme": (root / "README.md").is_file(),
            "agent_contract": (root / "AGENTS.md").is_file(),
            "normalizer_cli": (root / "implementation/tools/normalizer").is_file(),
            "architecture_contract": (root / "implementation/docs/architecture-three-modules.md").is_file(),
            "principles": (root / "implementation/docs/principles.md").is_file(),
            "maintenance_route": (root / "implementation/docs/agent-maintenance.md").is_file(),
        },
        "maintenance_contract": _maintenance_contract(root),
        "kernel_input_contract": _kernel_input_contract(root),
    }


def score(report: dict) -> dict:
    missing = len(report["required_paths_missing"])
    cycles = len(report["crate_cycles"])
    large_count = len(report["rust"]["files_over_3000_lines"])
    maintenance_violations = report.get("maintenance_contract", {}).get("violations", [])
    protocol_violations = report.get("kernel_input_contract", {}).get("violations", [])
    dimensions = {
        "boundary": 100 if cycles == 0 else 0,
        "discoverability": round(100 * (1 - missing / len(REQUIRED_PATHS)), 2),
        "file_size_debt": max(0, 100 - large_count * 5),
        "maintenance_contract": 100 if not maintenance_violations else 0,
        "kernel_input_contract": 100 if not protocol_violations else 0,
    }
    dimensions["overall"] = round(sum(dimensions.values()) / len(dimensions), 2)
    return dimensions


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--json", action="store_true", dest="as_json")
    args = parser.parse_args()
    report = collect()
    report["score"] = score(report)
    if args.as_json:
        print(json.dumps(report, ensure_ascii=False, indent=2, sort_keys=True))
        return 0
    print("Normalizer architecture health")
    print(f"overall score: {report['score']['overall']}/100")
    for name, value in report["score"].items():
        if name != "overall":
            print(f"{name}: {value}/100")
    print(f"Rust: {report['rust']['files']} files, {report['rust']['lines']} lines")
    print(f"Rust files over 3000 lines: {len(report['rust']['files_over_3000_lines'])}")
    print(f"Crate cycles: {', '.join(report['crate_cycles']) or 'none'}")
    print(f"Missing required paths: {', '.join(report['required_paths_missing']) or 'none'}")
    print(f"Maintenance contract violations: {len(report['maintenance_contract']['violations'])}")
    print(f"Kernel input contract violations: {len(report['kernel_input_contract']['violations'])}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
