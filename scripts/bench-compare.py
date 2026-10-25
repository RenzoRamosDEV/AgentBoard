#!/usr/bin/env python3
"""Compara los resultados de criterion con la línea base versionada.

Uso:
  python3 scripts/bench-compare.py capture --criterion-dir src-tauri/target/criterion \\
      --out baseline-candidate.json [--commit SHA] [--runner NOMBRE] [--profile PERFIL]
  python3 scripts/bench-compare.py compare --criterion-dir src-tauri/target/criterion \\
      --baseline docs/perf/baseline.json [--threshold 0.20]

Códigos de salida: 0 sin regresiones, 1 regresión o benchmark ausente,
2 entrada no válida (sin línea base, sin resultados, JSON mal formado).
"""

import argparse
import json
import sys
from datetime import datetime, timezone
from pathlib import Path

UPDATE_HINT = (
    "Si el cambio es intencionado, actualiza la línea base: descarga el artefacto "
    "`perf-baseline` de esta ejecución (`gh run download <id> -n perf-baseline`) "
    "y versiónalo como `docs/perf/baseline.json` en un commit propio "
    "(ver «Benchmarks en CI y línea base» en `CONTRIBUTING.md`)."
)


class InputError(Exception):
    pass


def read_json(path: Path):
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError) as err:
        raise InputError(f"JSON no válido en {path}: {err}") from err


def read_criterion(criterion_dir: Path) -> dict:
    results = {}
    for meta_path in sorted(Path(criterion_dir).glob("**/new/benchmark.json")):
        meta = read_json(meta_path)
        estimates = read_json(meta_path.with_name("estimates.json"))
        try:
            results[meta["full_id"]] = float(estimates["median"]["point_estimate"])
        except (KeyError, TypeError, ValueError) as err:
            raise InputError(f"resultado no válido en {meta_path.parent}: {err}") from err
    if not results:
        raise InputError(f"no hay resultados de criterion en {criterion_dir}")
    return results


def load_baseline(path: Path) -> dict:
    if not path.exists():
        raise InputError(
            f"no hay línea base: {path} no existe. Ejecuta el job `bench` en CI, "
            "descarga su artefacto `perf-baseline` (`gh run download <id> -n perf-baseline`) "
            f"y versiónalo como {path}."
        )
    doc = read_json(path)
    benchmarks = doc.get("benchmarks") if isinstance(doc, dict) else None
    if not isinstance(benchmarks, dict):
        raise InputError(f"línea base no válida en {path}: falta `benchmarks`")
    return doc


def cpu_model() -> str:
    try:
        for line in Path("/proc/cpuinfo").read_text().splitlines():
            if line.startswith("model name"):
                return line.split(":", 1)[1].strip()
    except OSError:
        pass
    return "desconocida"


def fmt_time(ns: float) -> str:
    for unit, scale in (("s", 1e9), ("ms", 1e6), ("µs", 1e3)):
        if ns >= scale:
            return f"{ns / scale:.2f} {unit}".replace(".", ",")
    return f"{ns:.2f} ns".replace(".", ",")


def fmt_pct(fraction: float) -> str:
    pct = round(fraction * 100, 2)
    if pct == 0:
        return "0 %"
    text = f"{pct:+.2f}".rstrip("0").rstrip(".")
    return f"{text} %".replace(".", ",")


def compare(baseline: dict, current: dict, threshold: float, cpu: str):
    expected = baseline["benchmarks"]
    captured = baseline.get("captured", {})
    rows, regressions, missing = [], [], []
    for name in sorted(expected.keys() | current.keys()):
        if name not in current:
            rows.append((name, fmt_time(expected[name]), "—", "—", "AUSENTE"))
            missing.append(
                f"AUSENTE {name}: está en la línea base y no en esta ejecución; "
                "actualiza la línea base."
            )
            continue
        if name not in expected:
            rows.append((name, "—", fmt_time(current[name]), "—", "sin línea base"))
            continue
        before, after = float(expected[name]), current[name]
        # Redondeo para que exactamente +20 % no falle por el error de coma flotante.
        change = round(after / before - 1, 9)
        status = "REGRESIÓN" if change > threshold else "ok"
        rows.append((name, fmt_time(before), fmt_time(after), fmt_pct(change), status))
        if status != "ok":
            regressions.append(
                f"REGRESIÓN {name}: {fmt_time(before)} → {fmt_time(after)} "
                f"({fmt_pct(change)}, umbral {fmt_pct(threshold)})"
            )

    lines = [
        "## Benchmarks frente a la línea base",
        "",
        f"Línea base: commit `{captured.get('commit', '?')}`, "
        f"runner `{captured.get('runner', '?')}`, CPU `{captured.get('cpu', '?')}`, "
        f"perfil `{captured.get('profile', '?')}`, {captured.get('date', '?')}. "
        f"Estadístico: mediana. Umbral: {fmt_pct(threshold)}.",
        "",
        "| Benchmark | Línea base | Actual | Cambio | Estado |",
        "|---|---|---|---|---|",
        *(f"| {' | '.join(row)} |" for row in rows),
    ]
    if captured.get("cpu") != cpu:
        lines += [
            "",
            f"> Aviso: CPU distinta de la de la línea base (`{captured.get('cpu', '?')}` → "
            f"`{cpu}`). Si el job falla solo por eso, vuelve a ejecutarlo; si se repite "
            "con esta CPU, regenera la línea base.",
        ]
    failures = regressions + missing
    if failures:
        lines += ["", *(f"- {failure}" for failure in failures), "", UPDATE_HINT]
    return (1 if failures else 0), "\n".join(lines) + "\n"


def cmd_capture(args) -> int:
    doc = {
        "version": 1,
        "statistic": "median",
        "unit": "ns",
        "captured": {
            "commit": args.commit,
            "date": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
            "runner": args.runner,
            "cpu": cpu_model(),
            "profile": args.profile,
        },
        "benchmarks": read_criterion(args.criterion_dir),
    }
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(doc, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print(f"Línea base candidata con {len(doc['benchmarks'])} benchmarks en {args.out}")
    return 0


def cmd_compare(args) -> int:
    baseline = load_baseline(args.baseline)
    current = read_criterion(args.criterion_dir)
    code, report = compare(baseline, current, args.threshold, cpu_model())
    print(report, end="")
    return code


def parse_args(argv):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = parser.add_subparsers(dest="command", required=True)

    capture = sub.add_parser("capture", help="escribe la línea base candidata")
    capture.add_argument("--criterion-dir", type=Path, required=True)
    capture.add_argument("--out", type=Path, required=True)
    capture.add_argument("--commit", default="desconocido")
    capture.add_argument("--runner", default="desconocido")
    capture.add_argument("--profile", default="bench-fast")
    capture.set_defaults(run=cmd_capture)

    comp = sub.add_parser("compare", help="compara con la línea base")
    comp.add_argument("--criterion-dir", type=Path, required=True)
    comp.add_argument("--baseline", type=Path, required=True)
    comp.add_argument("--threshold", type=float, default=0.20)
    comp.set_defaults(run=cmd_compare)
    return parser.parse_args(argv)


def main(argv=None) -> int:
    args = parse_args(argv)
    try:
        return args.run(args)
    except InputError as err:
        print(f"Error: {err}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
