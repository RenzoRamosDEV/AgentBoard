"""Tests del generador de fixtures sintéticos (`gen-fixtures.py`).

Uso: python3 -m unittest scripts/test_gen_fixtures.py
"""

import importlib.util
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).with_name("gen-fixtures.py")


def load_module():
    spec = importlib.util.spec_from_file_location("gen_fixtures", SCRIPT)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def run(out: Path, size: str, seed: int) -> None:
    subprocess.run(
        [sys.executable, str(SCRIPT), size, "--seed", str(seed), "--out", str(out)],
        check=True,
        capture_output=True,
    )


def tree(root: Path) -> dict:
    return {
        p.relative_to(root).as_posix(): p.read_bytes()
        for p in sorted(root.rglob("*"))
        if p.is_file()
    }


class ParseSize(unittest.TestCase):
    def setUp(self):
        self.gen = load_module()

    def test_sufijos_binarios(self):
        self.assertEqual(self.gen.parse_size("512k"), 512 * 1024)
        self.assertEqual(self.gen.parse_size("2m"), 2 * 1024**2)
        self.assertEqual(self.gen.parse_size("50M"), 50 * 1024**2)
        self.assertEqual(self.gen.parse_size("1g"), 1024**3)

    def test_valores_invalidos(self):
        for raw in ["", "abc", "10", "0m", "-1m", "1.5m", "3t"]:
            with self.subTest(raw=raw):
                with self.assertRaises(ValueError):
                    self.gen.parse_size(raw)

    def test_sesion_grande_solo_desde_50m(self):
        self.assertFalse(self.gen.wants_big_session(self.gen.parse_size("49m")))
        self.assertTrue(self.gen.wants_big_session(self.gen.parse_size("50m")))
        self.assertTrue(self.gen.wants_big_session(self.gen.parse_size("1g")))


class Generate(unittest.TestCase):
    SIZE = "2m"

    @classmethod
    def setUpClass(cls):
        cls.tmp = tempfile.TemporaryDirectory()
        base = Path(cls.tmp.name)
        cls.a, cls.b, cls.c = base / "a", base / "b", base / "c"
        run(cls.a, cls.SIZE, 7)
        run(cls.b, cls.SIZE, 7)
        run(cls.c, cls.SIZE, 8)
        cls.tree_a = tree(cls.a)

    @classmethod
    def tearDownClass(cls):
        cls.tmp.cleanup()

    def test_misma_semilla_mismo_arbol_byte_a_byte(self):
        self.assertEqual(self.tree_a, tree(self.b))

    def test_otra_semilla_otro_contenido(self):
        self.assertNotEqual(self.tree_a, tree(self.c))

    def test_tamano_total_dentro_del_5_por_ciento(self):
        target = 2 * 1024**2
        total = sum(len(v) for v in self.tree_a.values())
        self.assertLessEqual(abs(total - target), target * 0.05, total)

    def test_estructura_de_los_cinco_agentes(self):
        paths = list(self.tree_a)
        checks = {
            "claude_code": lambda p: p.startswith("claude_code/-home-dev-Proyectos-")
            and p.endswith(".jsonl"),
            "claude_code subagente": lambda p: p.startswith("claude_code/")
            and "/subagents/agent-" in p,
            "codex": lambda p: p.startswith("codex/sessions/")
            and Path(p).name.startswith("rollout-"),
            "copilot": lambda p: p.startswith("copilot/session-state/")
            and p.endswith("/events.jsonl"),
            "cursor": lambda p: p.startswith("cursor/projects/-home-dev-Proyectos-")
            and "/agent-transcripts/" in p,
            "gemini": lambda p: p.startswith("gemini/tmp/") and "/chats/" in p,
        }
        for name, check in checks.items():
            with self.subTest(agent=name):
                self.assertTrue(any(check(p) for p in paths), name)

    def test_todas_las_lineas_son_json(self):
        for path, data in self.tree_a.items():
            for n, line in enumerate(data.decode("utf-8").splitlines(), 1):
                with self.subTest(path=path, line=n):
                    json.loads(line)

    def test_sin_sesion_grande_por_debajo_de_50m(self):
        self.assertFalse(any("sesion-grande" in p for p in self.tree_a))

    def test_sesion_grande_de_5_mb(self):
        gen = load_module()
        with tempfile.TemporaryDirectory() as tmp:
            path = gen.write_big_session(Path(tmp), __import__("random").Random(1))
            size = path.stat().st_size
            self.assertIn("sesion-grande-", path.name)
            self.assertLessEqual(abs(size - 5 * 1024**2), 5 * 1024**2 * 0.05, size)


if __name__ == "__main__":
    unittest.main()
