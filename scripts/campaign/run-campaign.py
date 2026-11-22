#!/usr/bin/env python3
"""Campagne CH3/S4 — orchestrateur côté nœud A (PC, coordinateur).

Séquence d'une exécution de mesure (nœud A = ce PC, nœud B = worker QUIC
sur le téléphone Android, voir scripts/campaign/android/) :

  1. génère (ou réutilise) la clé Ed25519 du nœud A (a.key) et pousse la
     clé publique vers le téléphone (/sdcard/Download/campaign/) ;
  2. attend une offre de session fraîche du worker (offer.json : jeton à
     usage unique TTL 30 s, régénéré toutes les 20 s) et la récupère par
     adb pull (fraîcheur mesurée à l'horloge du téléphone) ;
  3. lance `quic-spike client --offer … --key-file a.key --mode ch3/<lien>`
     et capture le rapport JSON imprimé sur stdout ;
  4. répète (2)-(3) N fois par lien demandé ;
  5. agrège les exécutions dans results/ch3/<lien>/<date>/runs.json
     (min/médiane/max par métrique) et affiche le verdict contre les
     seuils §6.3 du plan Phase 0.

Liens :
  usb   — PC → téléphone via redirection adb forward tcp:7700 (à établir
          avec scripts/campaign/android/prepare-device.sh). Le worker doit
          annoncer 127.0.0.1:7700 (ADVERTISE=127.0.0.1:7700 bash worker-start.sh).
  wifi  — PC et téléphone sur le même réseau ; le worker annonce son IP
          Wi-Fi (auto-détectée par worker-start.sh).
  lan   — deuxième machine physique (non-Android) : voir spike-quic/README.md,
          ce script ne gère que les liens usb/wifi via adb.

Le nœud B n'est pas piloté à distance : l'opérateur lance worker-start.sh
dans Termux (une fois), puis ce script côté PC.

Usage :
  python3 scripts/campaign/run-campaign.py --links usb --runs 3
  python3 scripts/campaign/run-campaign.py --links usb,wifi --runs 3 --rtt-n 200

Outils requis : adb, python3 >= 3.9, quic-spike natif du PC
(`cargo build --release -p relay-quic-spike`), et le worker démarré sur
le téléphone.
"""

from __future__ import annotations

import argparse
import datetime as dt
import json
import os
import pathlib
import statistics
import subprocess
import sys
import time
from typing import NoReturn

# Binaire natif par défaut : .exe sous Windows.
DEFAULT_BIN = os.path.join("target", "release",
                           "quic-spike.exe" if os.name == "nt" else "quic-spike")

# ── Constantes du protocole (spike-quic) ────────────────────────────────────
OFFER_TTL_S = 30          # jeton de session à usage unique : TTL 30 s
OFFER_REGEN_S = 20        # le worker régénère l'offre toutes les 20 s
OFFER_STALE_S = 8         # au-delà, l'offre est considérée périmée
PULL_CADENCE_S = 2        # cadence de sondage de l'offre
DEV_CAMPAIGN_DIR = "/sdcard/Download/campaign"
DEV_OFFER = DEV_CAMPAIGN_DIR + "/offer.json"

# Seuils §6.3 (docs/plan-phase-0-relay-gpu.md) : LAN RTT p50 < 2 ms,
# VPN < 50 ms, setup de session < 1 s. Wi-Fi et USB n'ont pas de seuil
# dans le plan ; bornes internes prudentes (à ajuster après première mesure).
THRESHOLDS = {
    "usb": {"setup_s": 1.0, "rtt_p50_ms": 3.0},
    "wifi": {"setup_s": 1.0, "rtt_p50_ms": 10.0},
    "lan": {"setup_s": 1.0, "rtt_p50_ms": 2.0},
    "tailscale": {"setup_s": 1.0, "rtt_p50_ms": 50.0},
}
KNOWN_LINKS = ("usb", "wifi")


def sh(cmd, timeout=None) -> subprocess.CompletedProcess:
    return subprocess.run(
        cmd, capture_output=True, text=True, timeout=timeout, encoding="utf-8", errors="replace"
    )


def adb(args, timeout=None) -> subprocess.CompletedProcess:
    return sh([os.environ.get("ADB", "adb"), *args], timeout=timeout)


def die(msg: str) -> NoReturn:
    print(f"✗ {msg}", file=sys.stderr)
    sys.exit(1)


# ── Étape 1 : clé du coordinateur ────────────────────────────────────────────
def ensure_node_a_key(workdir: pathlib.Path, bin_path: pathlib.Path) -> str:
    key = workdir / "a.key"
    r = sh([str(bin_path), "pubkey", "--key-file", str(key)])
    if r.returncode != 0:
        die(f"quic-spike pubkey a échoué : {r.stderr.strip()}")
    return r.stdout.strip()


def push_client_pubkey(pubkey: str) -> None:
    local = os.path.join(os.environ.get("TEMP", "/tmp"), "ainon-campaign-client-pubkey.txt")
    with open(local, "w", encoding="utf-8") as f:
        f.write(pubkey + "\n")
    r = adb(["push", local, DEV_CAMPAIGN_DIR + "/client-pubkey.txt"], timeout=30)
    if r.returncode != 0:
        print(f"⚠ adb push client-pubkey : {r.stderr.strip()}")
    else:
        print(f"✓ clé publique du coordinateur poussée ({pubkey[:16]}…)")


# ── Étape 2 : offre fraîche ──────────────────────────────────────────────────
def device_offer_age_s() -> float | None:
    """Âge de l'offre sur le téléphone, à l'horloge DU téléphone (fiable
    même si PC et téléphone ne sont pas à l'heure)."""
    m = adb(["shell", "stat", "-c", "%Y", DEV_OFFER], timeout=10)
    n = adb(["shell", "date", "+%s"], timeout=10)
    if m.returncode != 0 or n.returncode != 0:
        return None
    try:
        mtime = int(m.stdout.strip())
        now = int(n.stdout.strip())
    except ValueError:
        return None
    return now - mtime


def pull_fresh_offer(local_offer: pathlib.Path, max_wait_s: float) -> bool:
    deadline = time.monotonic() + max_wait_s
    while time.monotonic() < deadline:
        age = device_offer_age_s()
        if age is not None and age < OFFER_STALE_S:
            r = adb(["pull", DEV_OFFER, str(local_offer)], timeout=15)
            if r.returncode == 0 and local_offer.exists():
                return True
        time.sleep(PULL_CADENCE_S)
    return False


def offer_endpoint(offer_path: pathlib.Path) -> str:
    try:
        offer = json.loads(offer_path.read_text(encoding="utf-8"))
        return str(offer.get("quic_endpoint", "?"))
    except Exception:
        return "?"


# ── Étapes 3-5 : exécutions et agrégation ────────────────────────────────────
def run_once(bin_path: pathlib.Path, offer: pathlib.Path, key: pathlib.Path,
             mode: str, rtt_n: int, bytes_: int) -> dict | None:
    r = sh(
        [str(bin_path), "client", "--offer", str(offer), "--key-file", str(key),
         "--mode", mode, "--rtt-n", str(rtt_n), "--bytes", str(bytes_)],
        timeout=OFFER_TTL_S + 5 * 60,
    )
    if r.returncode != 0:
        print(f"  ⚠ exécution échouée : {r.stderr.strip()[:200]}")
        return None
    try:
        return json.loads(r.stdout)
    except json.JSONDecodeError:
        print(f"  ⚠ sortie non-JSON : {r.stdout.strip()[:200]}")
        return None


def summarize(link: str, runs: list, rtt_n: int, out_dir: pathlib.Path,
              hardware: str) -> dict:
    def agg(key):
        vals = [r[key] for r in runs]
        return {"min": min(vals), "median": statistics.median(vals), "max": max(vals)}

    thr = THRESHOLDS[link]
    summary = {
        "link": link,
        "date": dt.datetime.now().isoformat(timespec="seconds"),
        "hardware": hardware,
        "runs": len(runs),
        "rtt_samples_per_run": rtt_n,
        "peer": runs[0].get("peer", "?"),
        "session_setup_ms": agg("session_setup_ms"),
        "rtt_p50_ms": agg("rtt_p50_ms"),
        "rtt_p95_ms": agg("rtt_p95_ms"),
        "transfer_bytes": runs[0].get("transfer_bytes"),
        "throughput_mbit_s": agg("throughput_mbit_s"),
        "thresholds_s63": thr,
        "verdict": {
            "setup_ok": max(r["session_setup_ms"] for r in runs) < thr["setup_s"] * 1e3,
            "rtt_p50_ok": statistics.median(
                [r["rtt_p50_ms"] for r in runs]) < thr["rtt_p50_ms"],
        },
        "runs_detail": runs,
    }
    out_dir.mkdir(parents=True, exist_ok=True)
    (out_dir / "runs.json").write_text(
        json.dumps(summary, indent=2, ensure_ascii=False), encoding="utf-8")
    return summary


def print_summary(s: dict, out_dir: pathlib.Path) -> None:
    print()
    print(f"═ Résumé {s['link']} — {s['runs']} exécutions, "
          f"{s['rtt_samples_per_run']} échantillons RTT/exécution")
    print(f"  pair          : {s['peer']}")
    print(f"  setup session : médiane {s['session_setup_ms']['median']:.2f} ms "
          f"(min {s['session_setup_ms']['min']:.2f} / max {s['session_setup_ms']['max']:.2f})")
    print(f"  RTT p50       : médiane {s['rtt_p50_ms']['median']:.3f} ms "
          f"(min {s['rtt_p50_ms']['min']:.3f} / max {s['rtt_p50_ms']['max']:.3f})")
    print(f"  RTT p95       : médiane {s['rtt_p95_ms']['median']:.3f} ms "
          f"(min {s['rtt_p95_ms']['min']:.3f} / max {s['rtt_p95_ms']['max']:.3f})")
    print(f"  Débit         : médiane {s['throughput_mbit_s']['median']:.0f} Mb/s "
          f"(min {s['throughput_mbit_s']['min']:.0f} / max {s['throughput_mbit_s']['max']:.0f})")
    thr, v = s["thresholds_s63"], s["verdict"]
    print(f"  Seuils §6.3   : setup < {thr['setup_s']:.0f} s → {'✓' if v['setup_ok'] else '✗'} ; "
          f"RTT p50 < {thr['rtt_p50_ms']:.0f} ms → {'✓' if v['rtt_p50_ok'] else '✗'}")
    print(f"  Écrit         : {out_dir / 'runs.json'}")


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--links", default="usb",
                    help="liens séparés par des virgules parmi usb,wifi (défaut : usb)")
    ap.add_argument("--runs", type=int, default=3,
                    help="exécutions par lien (défaut : 3)")
    ap.add_argument("--rtt-n", type=int, default=200,
                    help="échantillons RTT par exécution (défaut : 200)")
    ap.add_argument("--bytes", type=int, default=100 * 1024 * 1024,
                    help="octets du transfert de débit (défaut : 100 MiB)")
    ap.add_argument("--bin", default=os.environ.get(
        "QUIC_SPIKE_BIN", DEFAULT_BIN),
                    help="chemin du binaire quic-spike du PC")
    ap.add_argument("--hardware", default=os.environ.get("HARDWARE", "non renseigné"),
                    help="description libre du matériel (PC + téléphone)")
    ap.add_argument("--results-root", default="results/ch3")
    ap.add_argument("--offer-wait", type=float, default=90.0,
                    help="attente max d'une offre fraîche, en secondes")
    args = ap.parse_args()

    links = [l.strip().lower() for l in args.links.split(",") if l.strip()]
    bad = [l for l in links if l not in KNOWN_LINKS]
    if bad:
        die(f"liens inconnus : {bad} (connus : {list(KNOWN_LINKS)}) ; "
            f"pour un lien LAN entre deux PC, utilise directement quic-spike "
            f"serve/client (spike-quic/README.md)")

    bin_path = pathlib.Path(args.bin)
    if not bin_path.exists():
        die(f"{bin_path} introuvable — lance : cargo build --release -p relay-quic-spike "
            f"(ou passe --bin / QUIC_SPIKE_BIN)")

    # Le worker doit tourner : l'offre doit exister côté téléphone.
    if adb(["get-state"], timeout=10).returncode != 0:
        die("aucun appareil adb détecté — branche le téléphone (débogage USB activé)")

    workdir = pathlib.Path(".campaign")
    workdir.mkdir(exist_ok=True)
    a_key = workdir / "a.key"
    a_pub = ensure_node_a_key(workdir, bin_path)
    print(f"✓ clé du coordinateur : {a_pub[:16]}… ({a_key})")
    push_client_pubkey(a_pub)

    date_tag = dt.datetime.now().strftime("%Y%m%d-%H%M%S")
    overall = 0
    for link in links:
        print(f"\n▶ Lien « {link} » — {args.runs} exécutions")
        if link == "usb":
            fwd = adb(["forward", "tcp:7700", "tcp:7700"], timeout=10)
            if fwd.returncode != 0:
                print("  ⚠ adb forward tcp:7700 a échoué — la liaison USB ne passera pas")
            else:
                print("  ✓ adb forward tcp:7700 → téléphone :7700")

        runs = []
        for i in range(args.runs):
            offer = workdir / f"offer-{link}.json"
            print(f"  [{i + 1}/{args.runs}] attente d'une offre fraîche (< {OFFER_STALE_S} s)…")
            if not pull_fresh_offer(offer, args.offer_wait):
                print("  ✗ pas d'offre fraîche : worker arrêté ? "
                      "termux-setup-storage fait ? offre mirror active ?")
                break
            print(f"      offre {offer_endpoint(offer)} récupérée")
            report = run_once(bin_path, offer, a_key, f"ch3/{link}",
                              args.rtt_n, args.bytes)
            if report is not None:
                runs.append(report)
                print(f"      ✓ setup {report['session_setup_ms']:.2f} ms ; "
                      f"RTT p50 {report['rtt_p50_ms']:.3f} ms ; "
                      f"débit {report['throughput_mbit_s']:.0f} Mb/s")

        if not runs:
            print(f"  ✗ aucune exécution réussie pour « {link} »")
            overall = 1
            continue
        out_dir = pathlib.Path(args.results_root) / link / date_tag
        s = summarize(link, runs, args.rtt_n, out_dir, args.hardware)
        print_summary(s, out_dir)

    print("\nRappel protocole : note le matériel (--hardware), ≥ 3 exécutions par "
          "lien, et reporte les résultats comme spike-quic/results/loopback.json.")
    return overall


if __name__ == "__main__":
    sys.exit(main())
