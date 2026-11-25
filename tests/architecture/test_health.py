import importlib.util
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory


ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location(
    "normalizer_architecture_health", ROOT / "implementation/tools/architecture/health.py"
)
HEALTH = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(HEALTH)


class ProtocolScopeTests(unittest.TestCase):
    def test_obsolete_names_in_active_material_only(self):
        # Assemble the retired spellings so the regression's source remains
        # clean while its temporary fixtures exercise the actual matcher.
        schemas = "normalizer.normalized-"
        input_schema = schemas + "pure/1"
        result_schema = schemas + "behavior-result/1"
        term = "normalized" + "_pure"

        with TemporaryDirectory() as temporary:
            root = Path(temporary)
            for name, content in {
                "tests/e2e/stale.yaml": "schema: " + result_schema,
                "implementation/tools/scenarios/stale.mjs": "wire: " + input_schema,
                "usage/obsolete.md": term,
                "design/current.md": HEALTH.CANONICAL_PERS_SCHEMA,
                "implementation/tools/frontend/node_modules/stale.js": term,
                "implementation/packages/normalizer/crates/target/stale.rs": input_schema,
                "implementation/tools/architecture/health.py": input_schema,
            }.items():
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(content, encoding="utf-8")

            report = HEALTH._kernel_input_contract(root)
            self.assertEqual(report["active_materials"], 4)
            self.assertEqual(report["obsolete_active_paths"], [
                "implementation/tools/scenarios/stale.mjs", "tests/e2e/stale.yaml",
            ])
            problems = {
                item["path"]: item.get("expected")
                for item in report["violations"]
                if item["path"].endswith(("stale.mjs", "stale.yaml", "obsolete.md"))
            }
            self.assertEqual(problems, {
                "implementation/tools/scenarios/stale.mjs": HEALTH.CANONICAL_KERNEL_INPUT_SCHEMA,
                "tests/e2e/stale.yaml": HEALTH.CANONICAL_PERS_SCHEMA,
                "usage/obsolete.md": "kernel_input or kernel-input terminology",
            })


if __name__ == "__main__":
    unittest.main()
