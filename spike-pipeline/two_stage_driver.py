#!/usr/bin/env python3
"""Spike S4 — équivalence numérique 2 étages via pipeline_server (HTTP, 2 processus).

Compare la génération gloutonne d'un nœud unique (toutes les couches) avec une
chaîne node0 (couches [0, split)) -> node1 (couches [split, n_layer)), où l'état
caché transite en F32 base64 entre les deux processus. Stdlib uniquement.
"""
import argparse, base64, json, struct, sys, time, urllib.request

def post(base, path, body):
    req = urllib.request.Request(base + path, data=json.dumps(body).encode(),
                                 headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=60) as r:
        return json.loads(r.read())

def generate_single(base, ids, n, rid):
    out, step = [], "/prefill"
    cur = ids
    for _ in range(n):
        r = post(base, step, {"request_id": rid, "input_ids": cur, "seq_len": len(cur), "hidden_size": 0})
        out.append(r["next_token_id"]); cur = [r["next_token_id"]]; step = "/decode"
    post(base, "/clear", {"request_id": rid})
    return out

def generate_split(n0, n1, ids, n, rid):
    out, step, cur, hidden_bytes = [], "/prefill", ids, 0
    for _ in range(n):
        r0 = post(n0, step, {"request_id": rid, "input_ids": cur, "seq_len": len(cur), "hidden_size": 0})
        hidden_bytes += len(base64.b64decode(r0["hidden_states_b64"]))
        r1 = post(n1, step, {"request_id": rid, "hidden_states_b64": r0["hidden_states_b64"],
                             "seq_len": r0["seq_len"], "hidden_size": r0["hidden_size"]})
        out.append(r1["next_token_id"]); cur = [r1["next_token_id"]]; step = "/decode"
    for b in (n0, n1):
        post(b, "/clear", {"request_id": rid})
    return out, hidden_bytes

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--single", default="http://127.0.0.1:9100")
    ap.add_argument("--node0", default="http://127.0.0.1:9101")
    ap.add_argument("--node1", default="http://127.0.0.1:9102")
    ap.add_argument("--tokens", type=int, default=16)
    ap.add_argument("--prompt-ids", default="2,818,5279,529,7001,563")
    a = ap.parse_args()
    ids = [int(x) for x in a.prompt_ids.split(",")]
    ref = generate_single(a.single, ids, a.tokens, "ref")
    t = time.perf_counter()
    got, hb = generate_split(a.node0, a.node1, ids, a.tokens, "split")
    dt = time.perf_counter() - t
    first_diff = next((i for i, (x, y) in enumerate(zip(ref, got)) if x != y), None)
    res = {"prompt_ids": ids, "tokens": a.tokens, "single_node": ref, "two_stage": got,
           "identical": ref == got, "first_divergence": first_diff,
           "hidden_bytes_transferred": hb, "two_stage_wall_s": round(dt, 4)}
    print(json.dumps(res, indent=2))
    sys.exit(0 if ref == got else 1)

if __name__ == "__main__":
    main()
