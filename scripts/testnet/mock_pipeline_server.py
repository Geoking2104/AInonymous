#!/usr/bin/env python3
"""
Mock pipeline_server — SANS dépendance ML (stdlib only).

But : valider le PLOMBERIE distribuée d'AInonymous (plan statique, négociation
daemon↔daemon, session QUIC réutilisée, chaîne A→B, relais B→A→C, boucle de
décodage, purge KV-cache) sans torch/transformers ni modèle réel.

Implémente les mêmes endpoints que scripts/pipeline_server.py :
  /status /tokenize /detokenize /prefill /decode /clear

Comportement factice :
  - tokenize : renvoie une liste d'IDs fixe (longueur ~ nb de mots du prompt).
  - prefill/decode : nœud non-dernier → renvoie des "hidden states" bidon ;
    nœud dernier → renvoie un token. Le dernier nœud génère MAXGEN tokens puis
    EOS (id=1). L'état par request_id simule le KV-cache (vérifie /clear).
"""
import argparse
import base64
import json
import sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

CFG = {}
STATE = {}          # request_id -> nb de tokens déjà générés (mock KV-cache)
MAXGEN = 5          # le dernier nœud émet 5 tokens puis EOS

def log(*a):
    print("[mock]", *a, file=sys.stderr, flush=True)

class H(BaseHTTPRequestHandler):
    def _send(self, obj, code=200):
        body = json.dumps(obj).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *a):  # silence le logging HTTP par défaut
        pass

    def do_GET(self):
        if self.path == "/status":
            a = CFG["args"]
            self._send({
                "model_id": a.model, "layer_start": a.layer_start,
                "layer_end": a.layer_end, "total_layers": a.layer_end,
                "is_first_node": a.is_first_node, "is_last_node": a.is_last_node,
                "active_requests": len(STATE), "device": "cpu", "dtype": "mock",
            })
        else:
            self._send({"error": "not found"}, 404)

    def _read(self):
        n = int(self.headers.get("Content-Length", 0))
        return json.loads(self.rfile.read(n) or b"{}")

    def do_POST(self):
        a = CFG["args"]
        try:
            req = self._read()
        except Exception as e:
            return self._send({"error": f"bad json: {e}"}, 400)

        if self.path == "/tokenize":
            msgs = req.get("messages") or []
            text = " ".join(m.get("content", "") for m in msgs) if isinstance(msgs, list) else ""
            ids = list(range(2, 2 + max(1, len(text.split()))))
            return self._send({"input_ids": ids, "n_tokens": len(ids)})

        if self.path == "/detokenize":
            ids = req.get("token_ids", [])
            return self._send({"text": "".join(f" tok{t}" for t in ids)})

        if self.path in ("/prefill", "/decode"):
            rid = req.get("request_id", "?")
            hidden = base64.b64encode(b"\x00\x00\x00\x00").decode()  # 4 octets bidon
            seq_len = req.get("seq_len", 1)
            if not a.is_last_node:
                # nœud intermédiaire / premier : produit des hidden states
                return self._send({
                    "request_id": rid, "hidden_states_b64": hidden,
                    "seq_len": seq_len, "hidden_size": 8,
                    "next_token_id": None, "next_token_text": None,
                    "is_last_node": False,
                })
            # dernier nœud : produit un token
            n = STATE.get(rid, 0) + 1
            STATE[rid] = n
            if n >= MAXGEN:
                tid, txt = 1, ""        # EOS
            else:
                tid, txt = 100 + n, f" mot{n}"
            log(f"{self.path} {rid} -> token #{n} id={tid} (active={len(STATE)})")
            return self._send({
                "request_id": rid, "hidden_states_b64": None,
                "seq_len": 1, "hidden_size": 8,
                "next_token_id": tid, "next_token_text": txt,
                "is_last_node": True,
            })

        if self.path == "/clear":
            rid = req.get("request_id", "?")
            STATE.pop(rid, None)
            log(f"/clear {rid} -> KV-cache purgé (active={len(STATE)})")
            return self._send({"cleared": rid})

        self._send({"error": "not found"}, 404)


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--model", default="mock")
    p.add_argument("--port", type=int, default=9340)
    p.add_argument("--layer-start", type=int, default=0)
    p.add_argument("--layer-end", type=int, default=8)
    p.add_argument("--is-first-node", action="store_true")
    p.add_argument("--is-last-node", action="store_true")
    p.add_argument("--device", default="cpu")
    p.add_argument("--dtype", default="mock")
    args = p.parse_args()
    CFG["args"] = args
    log(f"démarrage :{args.port} couches[{args.layer_start},{args.layer_end}[ "
        f"first={args.is_first_node} last={args.is_last_node}")
    ThreadingHTTPServer(("127.0.0.1", args.port), H).serve_forever()


if __name__ == "__main__":
    main()
