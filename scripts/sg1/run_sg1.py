# -*- coding: utf-8 -*-
"""SG1 - two real Holochain 0.7 conductors, private-network admission.

End-to-end verification of the HybridNode admission model:

  * each conductor starts from a fresh data root and keystore;
  * each agent is admitted to the private DNA with a ``PrivateNetworkProof``
    signed by the network administrator whose key is baked into the DNA
    properties (``genesis_self_check``);
  * admission is refused without a proof, with a proof issued to another agent,
    with a proof for another network id, and with a proof signed by another
    administrator key;
  * a fresh agent with a valid proof is still admitted afterwards (control).

Everything is built from the repository sources: the zomes, the private DNA
variant, the app bundle and the ``private-network-proof`` tool. Nothing is
downloaded except the crates ``cargo`` needs.

Usage (from the repository root, with ``hc``/``holochain`` 0.7.0 on PATH):

    python3 scripts/sg1/run_sg1.py [--work-dir DIR] [--keep]

Exit code 0 means every assertion passed.
"""

import argparse
import base64
import json
import os
import pathlib
import shutil
import socket
import subprocess
import sys
import tempfile
import time

REPO = pathlib.Path(__file__).resolve().parents[2]
ROLE = "hybridnode-core-sg1"
TOOL_NAME = "private-network-proof.exe" if os.name == "nt" else "private-network-proof"


def log(msg):
    print(msg, flush=True)


def run(cmd, cwd=None, timeout=900, env=None):
    return subprocess.run(
        cmd, cwd=str(cwd or REPO), capture_output=True, text=True, timeout=timeout, env=env
    )


def which(name, env_var):
    explicit = os.environ.get(env_var)
    if explicit:
        return explicit
    found = shutil.which(name)
    if not found:
        raise SystemExit(f"{name} not found on PATH (set {env_var})")
    return found


def free_port():
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


def build_everything(work_dir):
    """Build the proof tool and the hybridnode zomes; return the wasm directory."""
    log("==> cargo build private-network-proof")
    target_dir = pathlib.Path(tempfile.gettempdir()) / "ainonymous-sg1-target"
    env = dict(os.environ)
    env["CARGO_TARGET_DIR"] = str(target_dir)
    r = run(["cargo", "build", "-p", "private-network-proof"], env=env)
    if r.returncode != 0:
        raise SystemExit(f"tool build failed:\n{r.stderr[-4000:]}")

    log("==> cargo build hybridnode zomes (wasm32-unknown-unknown)")
    r = run(
        [
            "cargo", "build", "--manifest-path", "dnas/hybridnode/Cargo.toml",
            "--release", "--target", "wasm32-unknown-unknown",
        ],
        env=env,
    )
    if r.returncode != 0:
        raise SystemExit(f"zome build failed:\n{r.stderr[-4000:]}")

    wasm_dir = target_dir / "wasm32-unknown-unknown" / "release"
    tool = target_dir / "debug" / TOOL_NAME
    if not tool.exists():
        raise SystemExit(f"proof tool missing at {tool}")
    return wasm_dir, tool


def write_private_dna(work_dir, wasm_dir, network_id, seed, admin_raw36_hex):
    zomes = work_dir / "dna" / "zomes"
    zomes.mkdir(parents=True, exist_ok=True)
    # cargo emits underscore names; DNA manifests use the dashed zome names.
    mapping = {
        "hybridnode_integrity.wasm": "hybridnode-integrity.wasm",
        "hybridnode_coordinator.wasm": "hybridnode-coordinator.wasm",
    }
    for src_name, dst_name in mapping.items():
        src = wasm_dir / src_name
        if not src.exists():
            # prebuilt bundles use the dashed zome names
            alt = wasm_dir / src_name.replace("_", "-")
            if alt.exists():
                src = alt
        if not src.exists():
            raise SystemExit(f"missing wasm {src}")
        shutil.copyfile(src, zomes / dst_name)

    props = ", ".join(str(b) for b in bytes.fromhex(admin_raw36_hex))
    (work_dir / "dna" / "workdir").mkdir(parents=True, exist_ok=True)
    (work_dir / "dna" / "workdir" / "dna.yaml").write_text(
        f"""---
manifest_version: "0"
name: hybridnode-core-sg1
integrity:
  network_seed: "{seed}"
  properties:
    private_network: true
    network_id: "{network_id}"
    network_admin_pubkey: [{props}]
  zomes:
    - name: hybridnode-integrity
      path: ../zomes/hybridnode-integrity.wasm
coordinator:
  zomes:
    - name: hybridnode-coordinator
      path: ../zomes/hybridnode-coordinator.wasm
      dependencies:
        - name: hybridnode-integrity
""",
        encoding="utf-8",
        newline="\n",
    )


def write_app(work_dir):
    app = work_dir / "app"
    app.mkdir(parents=True, exist_ok=True)
    (app / "happ.yaml").write_text(
        f"""---
manifest_version: "0"
name: sg1-hybridnode
description: "SG1 two-conductor private-network harness"
roles:
  - name: {ROLE}
    provisioning:
      strategy: create
      deferred: false
    dna:
      path: ../dna/workdir/hybridnode-core-sg1.dna
      clone_limit: 0
""",
        encoding="utf-8",
        newline="\n",
    )


def pack(hc, work_dir):
    log("==> hc dna pack / hc app pack")
    r = run([hc, "dna", "pack", str(work_dir / "dna" / "workdir")])
    if r.returncode != 0:
        raise SystemExit(f"dna pack failed:\n{r.stderr}")
    r = run([hc, "app", "pack", str(work_dir / "app")])
    if r.returncode != 0:
        raise SystemExit(f"app pack failed:\n{r.stderr}")
    return work_dir / "app" / "sg1-hybridnode.happ"


class Conductor:
    def __init__(self, name, work_dir, holochain, passphrase):
        self.name = name
        self.root = work_dir / f"node-{name}"
        self.root.mkdir(parents=True, exist_ok=True)
        self.port = free_port()
        config = self.root / "conductor.yaml"
        config.write_text(
            f"""---
data_root_path: {self.root}
keystore:
  type: lair_server_in_proc
  lair_root: {self.root / 'keystore'}
admin_interfaces:
  - driver:
      type: websocket
      port: {self.port}
      allowed_origins: "*"
""",
            encoding="utf-8",
            newline="\n",
        )
        self.log_file = open(self.root / "conductor.log", "w", encoding="utf-8")
        self.proc = subprocess.Popen(
            [holochain, "-c", str(config), "--piped"],
            stdin=subprocess.PIPE,
            stdout=self.log_file,
            stderr=subprocess.STDOUT,
            text=True,
        )
        self.passphrase = passphrase
        self.proc.stdin.write(f"{passphrase}\n{passphrase}\n")
        self.proc.stdin.flush()
        self.hc = None

    def hc_call(self, hc_bin, *args, timeout=180):
        return run([hc_bin, "client", "call", "--port", str(self.port), *args], timeout=timeout)

    def wait_ready(self, hc_bin, timeout=90):
        deadline = time.time() + timeout
        while time.time() < deadline:
            if self.hc_call(hc_bin, "list-apps").returncode == 0:
                return True
            time.sleep(1)
        return False

    def new_agent(self, hc_bin):
        r = self.hc_call(hc_bin, "new-agent")
        if r.returncode != 0:
            raise RuntimeError(f"new-agent failed: {r.stderr}")
        data = json.loads(r.stdout)
        key = (data[0] if isinstance(data, list) else str(data)).strip().strip('"')
        if len(key) != 53:
            raise RuntimeError(f"unexpected agent key {key!r}")
        return key

    def install(self, hc_bin, app_id, agent_key, roles=None, seed=None):
        args = ["install-app", str(HAPP), seed]
        if roles is not None:
            args.append(str(roles))
        args += ["--app-id", app_id, "--agent-key", agent_key]
        return self.hc_call(hc_bin, *args)

    def stop(self):
        try:
            self.proc.terminate()
            self.proc.wait(timeout=20)
        except Exception:
            try:
                self.proc.kill()
            except Exception:
                pass
        try:
            self.log_file.close()
        except Exception:
            pass


def make_proof(tool, work_dir, network_id, agent_key, seed_hex, tag):
    out = work_dir / f"proof-{tag}.b64"
    r = run(
        [
            str(tool), "proof", "--network-id", network_id, "--agent-key", agent_key,
            "--seed-hex", seed_hex, "--out", str(out),
        ]
    )
    if r.returncode != 0:
        raise RuntimeError(f"proof generation failed: {r.stderr}")
    return out


def roles_yaml(work_dir, proof_path, tag):
    """Role settings are a map role -> RoleSettingsYaml; bytes must be integers."""
    payload = base64.b64decode(proof_path.read_bytes().strip())
    numbers = ", ".join(str(b) for b in payload)
    path = work_dir / f"roles-{tag}.yaml"
    path.write_text(
        f"{ROLE}:\n  type: provisioned\n  membrane_proof: [{numbers}]\n",
        encoding="ascii",
        newline="\n",
    )
    return path


def main():
    parser = argparse.ArgumentParser(description="SG1 two-conductor admission test")
    parser.add_argument("--work-dir", default=None, help="keep artefacts here")
    parser.add_argument("--keep", action="store_true", help="do not delete the work dir")
    parser.add_argument(
        "--tool",
        default=None,
        help="prebuilt private-network-proof binary (skips the cargo build)",
    )
    parser.add_argument(
        "--wasm-dir",
        default=None,
        help="directory with prebuilt hybridnode-*.wasm (skips the zome build)",
    )
    args = parser.parse_args()

    hc_bin = which("hc", "HC_BIN")
    holochain_bin = which("holochain", "HOLOCHAIN_BIN")
    work_dir = pathlib.Path(args.work_dir or tempfile.mkdtemp(prefix="ainonymous-sg1-")).resolve()
    work_dir.mkdir(parents=True, exist_ok=True)

    results = []
    conductors = {}
    try:
        if args.tool and args.wasm_dir:
            tool = pathlib.Path(args.tool).resolve()
            wasm_dir = pathlib.Path(args.wasm_dir).resolve()
            if not tool.exists():
                raise SystemExit(f"prebuilt tool not found: {tool}")
            has_wasm = (wasm_dir / "hybridnode_integrity.wasm").exists() or (
                wasm_dir / "hybridnode-integrity.wasm"
            ).exists()
            if not has_wasm:
                raise SystemExit(f"prebuilt wasm not found in {wasm_dir}")
            log(f"==> prebuilt artefacts: {tool}, {wasm_dir}")
        else:
            wasm_dir, tool = build_everything(work_dir)

        admin_seed = os.urandom(32).hex()
        r = run([str(tool), "admin-pubkey", "--seed-hex", admin_seed])
        if r.returncode != 0:
            raise SystemExit(f"admin-pubkey failed: {r.stderr}")
        admin_raw36 = r.stdout.strip()

        network_id = f"sg1-{os.urandom(4).hex()}"
        seed = f"sg1-seed-{os.urandom(4).hex()}"
        log(f"==> network_id={network_id}")
        write_private_dna(work_dir, wasm_dir, network_id, seed, admin_raw36)
        write_app(work_dir)
        happ = pack(hc_bin, work_dir)
        global HAPP
        HAPP = happ

        passphrase = f"sg1-{os.urandom(6).hex()}"
        for name in ("A", "B"):
            conductors[name] = Conductor(name, work_dir, holochain_bin, passphrase)
        for name, c in conductors.items():
            if not c.wait_ready(hc_bin):
                raise RuntimeError(f"conductor {name} never answered on {c.port}")

        keys = {name: c.new_agent(hc_bin) for name, c in conductors.items()}
        for name, key in keys.items():
            log(f"[{name}] agent {key}")

        for name, c in conductors.items():
            proof = make_proof(tool, work_dir, network_id, keys[name], admin_seed, f"valid-{name}")
            roles = roles_yaml(work_dir, proof, f"valid-{name}")
            r = c.install(hc_bin, f"sg1-{name.lower()}", keys[name], roles, seed)
            results.append((f"valid proof admitted on node {name}", r.returncode == 0, r.stderr))
            if r.returncode == 0:
                listed = f"sg1-{name.lower()}" in c.hc_call(hc_bin, "list-apps").stdout
                results.append((f"app listed on node {name}", listed, ""))

        a = conductors["A"]
        victim = a.new_agent(hc_bin)
        other = a.new_agent(hc_bin)
        proof_other = make_proof(tool, work_dir, network_id, other, admin_seed, "other")

        r = a.install(hc_bin, "sg1-noproof", victim, None, seed)
        results.append((
            "install without proof refused",
            r.returncode != 0 and "membrane proof" in (r.stderr + r.stdout).lower(),
            r.stderr or r.stdout,
        ))

        r = a.install(hc_bin, "sg1-wrongagent", victim, roles_yaml(work_dir, proof_other, "wrong-agent"), seed)
        results.append((
            "proof for another agent refused",
            r.returncode != 0 and "not issued to this agent" in (r.stderr + r.stdout),
            r.stderr or r.stdout,
        ))

        other_net = make_proof(tool, work_dir, f"{network_id}-other", victim, admin_seed, "wrong-net")
        r = a.install(hc_bin, "sg1-wrongnet", victim, roles_yaml(work_dir, other_net, "wrong-net"), seed)
        results.append((
            "proof for another network refused",
            r.returncode != 0 and "different network" in (r.stderr + r.stdout),
            r.stderr or r.stdout,
        ))

        rogue = make_proof(tool, work_dir, network_id, victim, "ff" * 32, "rogue")
        r = a.install(hc_bin, "sg1-rogue", victim, roles_yaml(work_dir, rogue, "rogue"), seed)
        results.append((
            "proof signed by another admin key refused",
            r.returncode != 0 and "signature does not match" in (r.stderr + r.stdout),
            r.stderr or r.stdout,
        ))

        control = a.new_agent(hc_bin)
        proof_control = make_proof(tool, work_dir, network_id, control, admin_seed, "control")
        r = a.install(hc_bin, "sg1-control", control, roles_yaml(work_dir, proof_control, "control"), seed)
        results.append(("control: fresh agent with valid proof admitted", r.returncode == 0, r.stderr))

    finally:
        for c in conductors.values():
            c.stop()
        if not results:
            import traceback
            traceback.print_exc()
        log("\n=== SG1 RESULTS ===")
        failures = 0
        for name, ok, detail in results:
            log(f"{'PASS' if ok else 'FAIL'}  {name}")
            if not ok:
                failures += 1
                log(f"      {(detail or '').strip()[:600]}")
        log(f"\n{len(results) - failures}/{len(results)} assertions passed")
        if not args.keep and not args.work_dir:
            shutil.rmtree(work_dir, ignore_errors=True)
        else:
            log(f"artefacts kept in {work_dir}")
        if not results:
            failures = 1
        sys.exit(1 if failures else 0)


HAPP = None

if __name__ == "__main__":
    main()
