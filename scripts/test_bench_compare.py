"""Tests del comparador de benchmarks (`bench-compare.py`).

Uso: python3 -m unittest scripts/test_bench_compare.py

Los resultados de criterion se simulan con árboles falsos en un directorio
temporal: `<grupo>/<función>/{new,base,change}/{benchmark,estimates}.json`.
`base/` y `change/` llevan valores señuelo que nunca deben leerse.
"""

import contextlib
import importlib.util
import io
import json
import os
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).with_name("bench-compare.py")
GOLDEN = Path(__file__).parent / "__golden__" / "bench-report.md"


def load_module():
    spec = importlib.util.spec_from_file_location("bench_compare", SCRIPT)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def write_bench(root: Path, full_id: str, median_ns: float, directory: str = None):
    bench_dir = root / (directory or full_id)
    for kind, value in (("new", median_ns), ("base", 1.0), ("change", 2.0)):
        d = bench_dir / kind
        d.mkdir(parents=True, exist_ok=True)
        (d / "benchmark.json").write_text(
            json.dumps({"full_id": full_id, "directory_name": directory or full_id})
        )
        (d / "estimates.json").write_text(
            json.dumps(
                {
                    "mean": {"point_estimate": value * 3},
                    "median": {"point_estimate": value},
                }
            )
        )


def baseline_doc(benchmarks: dict, cpu: str = "CPU A") -> dict:
    return {
        "version": 1,
        "statistic": "median",
        "unit": "ns",
        "captured": {
            "commit": "abc1234",
            "date": "2026-10-25T10:00:00Z",
            "runner": "ubuntu-22.04",
            "cpu": cpu,
            "profile": "bench-fast",
        },
        "benchmarks": benchmarks,
    }


class Base(unittest.TestCase):
    def setUp(self):
        self.mod = load_module()
        self._tmp = tempfile.TemporaryDirectory()
        self.tmp = Path(self._tmp.name)
        self.crit = self.tmp / "criterion"
        self.crit.mkdir()

    def tearDown(self):
        self._tmp.cleanup()

    def write_baseline(self, doc) -> Path:
        path = self.tmp / "baseline.json"
        path.write_text(doc if isinstance(doc, str) else json.dumps(doc))
        return path

    def run_main(self, *argv):
        out, err = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            code = self.mod.main(list(argv))
        return code, out.getvalue(), err.getvalue()

    def run_compare(self, baseline: Path):
        return self.run_main(
            "compare", "--criterion-dir", str(self.crit), "--baseline", str(baseline)
        )


class ReadCriterion(Base):
    def test_lee_la_mediana_de_new_por_full_id(self):
        write_bench(self.crit, "parse_line/claude_code", 2410.5)
        write_bench(
            self.crit,
            "summary_queries/breakdown(project)",
            158000.0,
            directory="summary_queries/breakdown_project_",
        )

        self.assertEqual(
            self.mod.read_criterion(self.crit),
            {
                "parse_line/claude_code": 2410.5,
                "summary_queries/breakdown(project)": 158000.0,
            },
        )

    def test_carpeta_sin_resultados_es_entrada_no_valida(self):
        with self.assertRaises(self.mod.InputError):
            self.mod.read_criterion(self.crit)

    def test_carpeta_inexistente_es_entrada_no_valida(self):
        with self.assertRaises(self.mod.InputError):
            self.mod.read_criterion(self.tmp / "no-existe")


class CompareThreshold(Base):
    BASE = 1000.0

    def test_mejora_igual_y_bajo_el_umbral_pasan(self):
        for actual in (500.0, 1000.0, 1199.9, 1200.0):
            with self.subTest(actual=actual):
                write_bench(self.crit, "summary_queries/activity", actual)
                baseline = self.write_baseline(
                    baseline_doc({"summary_queries/activity": self.BASE})
                )
                code, out, _ = self.run_compare(baseline)
                self.assertEqual(code, 0, out)
                self.assertNotIn("REGRESIÓN", out)

    def test_exactamente_20_por_ciento_con_decimales_no_exactos_pasa(self):
        write_bench(self.crit, "parse_line/claude_code", 2892.6)
        baseline = self.write_baseline(baseline_doc({"parse_line/claude_code": 2410.5}))
        code, out, _ = self.run_compare(baseline)
        self.assertEqual(code, 0, out)

    def test_por_encima_del_umbral_falla_nombrando_valores_y_porcentaje(self):
        cases = [
            (1200.1, "REGRESIÓN summary_queries/activity: 1,00 µs → 1,20 µs (+20,01 %, umbral +20 %)"),
            (1250.0, "REGRESIÓN summary_queries/activity: 1,00 µs → 1,25 µs (+25 %, umbral +20 %)"),
        ]
        for actual, line in cases:
            with self.subTest(actual=actual):
                write_bench(self.crit, "summary_queries/activity", actual)
                baseline = self.write_baseline(
                    baseline_doc({"summary_queries/activity": self.BASE})
                )
                code, out, _ = self.run_compare(baseline)
                self.assertEqual(code, 1)
                self.assertIn(line, out)
                self.assertIn("perf-baseline", out)

    def test_umbral_configurable(self):
        write_bench(self.crit, "summary_queries/activity", 1150.0)
        baseline = self.write_baseline(
            baseline_doc({"summary_queries/activity": self.BASE})
        )
        code, out, _ = self.run_main(
            "compare",
            "--criterion-dir",
            str(self.crit),
            "--baseline",
            str(baseline),
            "--threshold",
            "0.10",
        )
        self.assertEqual(code, 1)
        self.assertIn("(+15 %, umbral +10 %)", out)


class CompareCompleteness(Base):
    def test_bench_de_la_linea_base_ausente_falla_y_pide_actualizarla(self):
        write_bench(self.crit, "parse_line/codex", 100.0)
        baseline = self.write_baseline(
            baseline_doc({"parse_line/codex": 100.0, "parse_line/gemini": 100.0})
        )
        code, out, _ = self.run_compare(baseline)
        self.assertEqual(code, 1)
        self.assertIn("AUSENTE parse_line/gemini", out)
        self.assertIn("actualiza la línea base", out)

    def test_bench_nuevo_sin_linea_base_no_falla(self):
        write_bench(self.crit, "parse_line/codex", 100.0)
        write_bench(self.crit, "parse_line/nuevo", 5000.0)
        baseline = self.write_baseline(baseline_doc({"parse_line/codex": 100.0}))
        code, out, _ = self.run_compare(baseline)
        self.assertEqual(code, 0, out)
        self.assertIn("| parse_line/nuevo | — | 5,00 µs | — | sin línea base |", out)


class CompareInvalidInput(Base):
    def test_sin_archivo_de_linea_base(self):
        write_bench(self.crit, "parse_line/codex", 100.0)
        code, _, err = self.run_compare(self.tmp / "baseline.json")
        self.assertEqual(code, 2)
        self.assertIn("no hay línea base", err)
        self.assertIn("perf-baseline", err)

    def test_carpeta_sin_resultados(self):
        baseline = self.write_baseline(baseline_doc({"parse_line/codex": 100.0}))
        code, _, err = self.run_compare(baseline)
        self.assertEqual(code, 2)
        self.assertIn("no hay resultados de criterion", err)

    def test_json_mal_formado(self):
        write_bench(self.crit, "parse_line/codex", 100.0)
        cases = {
            "linea base truncada": ("{", None),
            "linea base sin benchmarks": (json.dumps({"version": 1}), None),
            "estimates truncado": (json.dumps(baseline_doc({"parse_line/codex": 1.0})), "{"),
            "estimates sin mediana": (
                json.dumps(baseline_doc({"parse_line/codex": 1.0})),
                json.dumps({"mean": {"point_estimate": 1.0}}),
            ),
        }
        for name, (baseline_text, estimates_text) in cases.items():
            with self.subTest(name):
                write_bench(self.crit, "parse_line/codex", 100.0)
                if estimates_text is not None:
                    (self.crit / "parse_line/codex/new/estimates.json").write_text(
                        estimates_text
                    )
                code, _, err = self.run_compare(self.write_baseline(baseline_text))
                self.assertEqual(code, 2)
                self.assertIn("no válid", err)


class CpuWarning(Base):
    def test_cpu_distinta_avisa_sin_fallar(self):
        doc = baseline_doc({"parse_line/codex": 100.0}, cpu="CPU A")
        code, report = self.mod.compare(doc, {"parse_line/codex": 100.0}, 0.20, "CPU B")
        self.assertEqual(code, 0)
        self.assertIn("CPU distinta", report)
        self.assertIn("CPU A", report)
        self.assertIn("CPU B", report)

    def test_misma_cpu_no_avisa(self):
        doc = baseline_doc({"parse_line/codex": 100.0}, cpu="CPU A")
        _, report = self.mod.compare(doc, {"parse_line/codex": 100.0}, 0.20, "CPU A")
        self.assertNotIn("CPU distinta", report)


class Capture(Base):
    def test_escribe_la_linea_base_y_comparar_contra_ella_pasa(self):
        write_bench(self.crit, "parse_line/claude_code", 2410.5)
        write_bench(self.crit, "summary_queries/breakdown(project)", 158000.0)
        out_path = self.tmp / "out" / "candidate.json"

        code, _, _ = self.run_main(
            "capture",
            "--criterion-dir",
            str(self.crit),
            "--out",
            str(out_path),
            "--commit",
            "deadbeef",
            "--runner",
            "ubuntu-22.04",
            "--profile",
            "bench-fast",
        )

        self.assertEqual(code, 0)
        doc = json.loads(out_path.read_text())
        self.assertEqual(doc["version"], 1)
        self.assertEqual(doc["statistic"], "median")
        self.assertEqual(doc["unit"], "ns")
        self.assertEqual(doc["captured"]["commit"], "deadbeef")
        self.assertEqual(doc["captured"]["runner"], "ubuntu-22.04")
        self.assertEqual(doc["captured"]["profile"], "bench-fast")
        self.assertIn("cpu", doc["captured"])
        self.assertRegex(doc["captured"]["date"], r"^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ$")
        self.assertEqual(
            doc["benchmarks"],
            {
                "parse_line/claude_code": 2410.5,
                "summary_queries/breakdown(project)": 158000.0,
            },
        )
        self.assertEqual(self.run_compare(out_path)[0], 0)

    def test_sin_resultados_no_escribe_nada(self):
        out_path = self.tmp / "candidate.json"
        code, _, err = self.run_main(
            "capture", "--criterion-dir", str(self.crit), "--out", str(out_path)
        )
        self.assertEqual(code, 2)
        self.assertFalse(out_path.exists())
        self.assertIn("no hay resultados de criterion", err)


class GoldenReport(Base):
    def test_informe_markdown(self):
        doc = baseline_doc(
            {
                "ingest_file/claude_code_5mib": 41_200_000.0,
                "parse_line/claude_code": 2410.5,
                "parse_line/gone": 900.0,
                "summary_queries/activity": 5_410_000.0,
                "summary_queries/breakdown(project)": 158_000.0,
            },
            cpu="AMD EPYC 7763 64-Core Processor",
        )
        current = {
            "ingest_file/claude_code_5mib": 39_000_000.0,
            "parse_line/claude_code": 2892.6,
            "parse_line/nuevo": 850.0,
            "summary_queries/activity": 6_892_340.0,
            "summary_queries/breakdown(project)": 158_000.0,
        }

        code, report = self.mod.compare(
            doc, current, 0.20, "Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz"
        )

        self.assertEqual(code, 1)
        if os.environ.get("UPDATE_GOLDEN") == "1":
            GOLDEN.parent.mkdir(exist_ok=True)
            GOLDEN.write_text(report)
        self.assertEqual(report, GOLDEN.read_text())


if __name__ == "__main__":
    unittest.main()
