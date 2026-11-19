#!/usr/bin/env python3
"""Agrège les résultats de campagne CH3/S4 vers le format results/ du dépôt.

Lit les runs.json produits par run-campaign.py (results/ch3/<lien>/<date>/)
et un éventuel results/ch3/hardware.json (descriptif du matériel), puis :

  1. écrit results/ch3/campaign-<date>.json au même format que
     spike-quic/results/loopback.json (date, machine, commande, runs[]) —
     les exécutions brutes y sont copiées telles quelles, mode inclus ;
  2. imprime un tableau Markdown prêt à coller dans un README ou un rapport
     (une ligne par lien : setup / RTT p50 / p95 / débit, min–max).

Usage :
  python3 scripts/campaign/aggregate-results.py                    # tout results/ch3/
  python3 scripts/campaign/aggregate-results.py results/ch3/usb/20261119-*
  python3 scripts/campaign/aggregate-results.py --hardware-json results/ch3/hardware.json
"""

from __future__ import annotations

import argparse
import datetime as dt
import json
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parents[2]
DEFAULT_ROOT = ROOT / "results" / "ch3"


def load_summary(p: pathlib.Path) -> dict:
    return json.loads(p.read_text(encoding="utf-8"))


def fmt_range(v: dict, digits: int) -> str:
    return f"{v['median']:.{digits}f} ({v['min']:.{digits}f}–{v['max']:.{digits}f})"


def markdown_table(summaries: list[dict]) -> str:
    lines = [
        "| Lien | Setup session (ms) | RTT p50 (ms) | RTT p95 (ms) | Débit (Mb/s) | Seuils §6.3 |",
        "| --- | --- | --- | --- | --- | --- |",
    ]
    for s in summaries:
        thr = s["thresholds_s63"]
        verdict = s["verdict"]
        ok = "✓" if (verdict["setup_ok"] and verdict["rtt_p50_ok"]) else "✗"
        lines.append(
            f"| {s['link']} | {fmt_range(s['session_setup_ms'], 2)} "
            f"| {fmt_range(s['rtt_p50_ms'], 3)} "
            f"| {fmt_range(s['rtt_p95_ms'], 3)} "
            f"| {fmt_range(s['throughput_mbit_s'], 0)} "
            f"| {ok} (setup < {thr['setup_s']:.0f} s, RTT p50 < {thr['rtt_p50_ms']:.0f} ms) |"
        )
    return "\n".join(lines)


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("paths", nargs="*",
                    help="runs.json ou répertoires les contenant (défaut : results/ch3/**/runs.json)")
    ap.add_argument("--hardware-json", type=pathlib.Path,
                    default=DEFAULT_ROOT / "hardware.json",
                    help="descriptif du matériel (défaut : results/ch3/hardware.json)")
    ap.add_argument("--out", type=pathlib.Path, default=None,
                    help="fichier de sortie (défaut : results/ch3/campaign-<date>.json)")
    args = ap.parse_args()

    # Collecte des runs.json
    files: list[pathlib.Path] = []
    if args.paths:
        for p in map(pathlib.Path, args.paths):
            if p.is_file() and p.name == "runs.json":
                files.append(p)
            elif p.is_dir():
                files.extend(sorted(p.rglob("runs.json")))
    else:
        files = sorted(DEFAULT_ROOT.rglob("runs.json"))
    if not files:
        print("✗ aucun runs.json trouvé — lance d'abord run-campaign.py", file=sys.stderr)
        return 1

    summaries = [load_summary(p) for p in files]
    # Un résumé par (lien, date) : les doublons écrasés gardent le plus récent.
    by_link_date: dict[tuple, dict] = {}
    for p, s in zip(files, summaries):
        by_link_date[(s["link"], s["date"])] = s
    summaries = list(by_link_date.values())

    machine = "non renseigné"
    if args.hardware_json and args.hardware_json.exists():
        try:
            machine = json.loads(args.hardware_json.read_text(encoding="utf-8")).get(
                "hardware", machine)
        except (json.JSONDecodeError, OSError):
            pass

    date_tag = dt.datetime.now().strftime("%Y-%m-%d")
    campaign = {
        "date": date_tag,
        "machine": machine,
        "commande": "scripts/campaign (CH3/S4) — quic-spike serve/client, cf. docs/campaign/PROTOCOL.md",
        "runs": [r for s in summaries for r in s["runs_detail"]],
        "per_link_summary": [
            {k: v for k, v in s.items() if k != "runs_detail"} for s in summaries
        ],
    }

    out = args.out or (DEFAULT_ROOT / f"campaign-{date_tag}.json")
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(campaign, indent=2, ensure_ascii=False), encoding="utf-8")

    print(f"✓ {out}  ({len(campaign['runs'])} exécutions, "
          f"{len(summaries)} résumés de lien)")
    print()
    print(markdown_table(summaries))
    return 0


if __name__ == "__main__":
    sys.exit(main())
