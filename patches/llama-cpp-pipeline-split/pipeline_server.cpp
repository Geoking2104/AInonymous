// Minimal HTTP pipeline-split server (PoC), matching ainonymous-daemon's
// PipelineClient interface (crates/ainonymous-daemon/src/pipeline_client.rs)
// field-for-field, so it could act as a drop-in alternative backend to
// pipeline_server.py for a 2-node topology.
//
// Scope (see patches/llama-cpp-pipeline-split/README.md for full limitations):
//   - Gemma3 Dense only (this session's patch target)
//   - Exactly 2 nodes verified so far: a "first" node (--layer-start 0) and a
//     "last" node (--layer-start N, runs to real completion / real logits).
//     A real middle node (layer_start>0 AND layer_end<n_layer) is supported by
//     this same code path but has only been tested on a 2-layer model (no room
//     for a real middle node there) -- see README section 5.
//   - Single in-flight sequence per server process (no concurrent request slots).
//   - Greedy decoding only (argmax), no sampling parameters.
//   - Hidden states serialized as F32 base64 (NOT F16 like scripts/pipeline_server.py --
//     don't mix a native node with a Python node in the same chain, the wire format differs).
//
// CLI is a superset of scripts/pipeline_server.py's, so this binary can be dropped
// into run_testnet_2.sh (BACKEND=native) with the same --layer-end/--is-first-node/
// --is-last-node flags. --device/--dtype are accepted and ignored (CPU/F32 only here).
//
// Endpoints (mirrors pipeline_client.rs):
//   GET  /status
//   POST /prefill   {request_id, input_ids? , hidden_states_b64?, seq_len, hidden_size}
//   POST /decode     (same shape as /prefill)
//   POST /clear     {request_id}
//   POST /tokenize   {messages: [...]}  -> {input_ids: [...]}
//   POST /detokenize {token_ids: [...]} -> {text: "..."}

#include "llama.h"
#include "llama-ext.h"
#include "httplib.h"
#include "json.hpp"

#include <cstdio>
#include <cstring>
#include <string>
#include <vector>
#include <mutex>
#include <algorithm>

using json = nlohmann::json;

// ---------- base64 ----------
static const char B64_CHARS[] = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

static std::string b64_encode(const uint8_t * data, size_t len) {
    std::string out;
    out.reserve(((len + 2) / 3) * 4);
    size_t i = 0;
    while (i + 3 <= len) {
        uint32_t n = (data[i] << 16) | (data[i+1] << 8) | data[i+2];
        out += B64_CHARS[(n >> 18) & 0x3F];
        out += B64_CHARS[(n >> 12) & 0x3F];
        out += B64_CHARS[(n >> 6) & 0x3F];
        out += B64_CHARS[n & 0x3F];
        i += 3;
    }
    size_t rem = len - i;
    if (rem == 1) {
        uint32_t n = data[i] << 16;
        out += B64_CHARS[(n >> 18) & 0x3F];
        out += B64_CHARS[(n >> 12) & 0x3F];
        out += "==";
    } else if (rem == 2) {
        uint32_t n = (data[i] << 16) | (data[i+1] << 8);
        out += B64_CHARS[(n >> 18) & 0x3F];
        out += B64_CHARS[(n >> 12) & 0x3F];
        out += B64_CHARS[(n >> 6) & 0x3F];
        out += "=";
    }
    return out;
}

static std::vector<uint8_t> b64_decode(const std::string & in) {
    static int8_t T[256];
    static bool init = false;
    if (!init) {
        std::fill(std::begin(T), std::end(T), -1);
        for (int i = 0; i < 64; ++i) T[(uint8_t)B64_CHARS[i]] = (int8_t) i;
        init = true;
    }
    std::vector<uint8_t> out;
    out.reserve(in.size() / 4 * 3);
    int val = 0, bits = -8;
    for (unsigned char c : in) {
        if (c == '=' || T[c] == -1) { if (c == '=') break; else continue; }
        val = (val << 6) + T[c];
        bits += 6;
        if (bits >= 0) {
            out.push_back((uint8_t)((val >> bits) & 0xFF));
            bits -= 8;
        }
    }
    return out;
}

static std::vector<float> bytes_to_floats(const std::vector<uint8_t> & b) {
    std::vector<float> out(b.size() / sizeof(float));
    memcpy(out.data(), b.data(), out.size() * sizeof(float));
    return out;
}

static std::vector<uint8_t> floats_to_bytes(const std::vector<float> & f) {
    std::vector<uint8_t> out(f.size() * sizeof(float));
    memcpy(out.data(), f.data(), out.size());
    return out;
}

// ---------- server state ----------
struct ServerConfig {
    std::string model_path;
    int32_t layer_start = 0;
    int32_t split = 0; // for non-last nodes: layer index to tap via embeddings_layer_inp (== --layer-end)
    int port = 9340;
    bool is_last_node = false;
    bool is_first_node_flag = false;   // explicitly set via --is-first-node
    bool is_first_node_flag_set = false;
};

struct ServerState {
    llama_model * model = nullptr;
    const llama_vocab * vocab = nullptr;
    llama_context * ctx = nullptr;
    int32_t n_vocab = 0;
    int32_t n_embd = 0;
    int32_t n_layer = 0;
    ServerConfig cfg;
    std::mutex mu;
    int32_t next_pos = 0;
    std::string cur_request_id;
    int active_requests = 0;
};

static ServerState S;

static bool is_first_node() {
    if (S.cfg.is_first_node_flag_set) return S.cfg.is_first_node_flag;
    return S.cfg.layer_start == 0;
}

// runs one forward step (prefill or decode -- same code path) for n tokens starting at S.next_pos.
// returns hidden_states (if not last node) or fills out_next_token (if last node).
struct StepResult {
    std::vector<float> hidden; // valid if !is_last_node
    int32_t next_token = -1;   // valid if is_last_node
};

static StepResult run_step(const std::vector<int32_t> * input_ids, const std::vector<float> * hidden_in, int32_t n_tokens) {
    StepResult res;

    if (is_first_node()) {
        llama_batch b = llama_batch_init(n_tokens, 0, 1);
        b.n_tokens = n_tokens;
        for (int i = 0; i < n_tokens; ++i) {
            b.token[i] = (*input_ids)[i];
            b.pos[i] = S.next_pos + i;
            b.n_seq_id[i] = 1;
            b.seq_id[i][0] = 0;
            b.logits[i] = 1;
        }
        if (llama_decode(S.ctx, b) != 0) { llama_batch_free(b); throw std::runtime_error("decode failed (first node)"); }
        llama_batch_free(b);
    } else {
        llama_batch b = llama_batch_init(n_tokens, S.n_embd, 1);
        b.n_tokens = n_tokens;
        memcpy(b.embd, hidden_in->data(), sizeof(float) * n_tokens * S.n_embd);
        for (int i = 0; i < n_tokens; ++i) {
            b.pos[i] = S.next_pos + i;
            b.n_seq_id[i] = 1;
            b.seq_id[i][0] = 0;
            b.logits[i] = 1;
        }
        if (llama_decode(S.ctx, b) != 0) { llama_batch_free(b); throw std::runtime_error("decode failed (non-first node)"); }
        llama_batch_free(b);
    }

    S.next_pos += n_tokens;

    if (S.cfg.is_last_node) {
        const float * logits = llama_get_logits_ith(S.ctx, n_tokens - 1);
        if (!logits) throw std::runtime_error("no logits");
        res.next_token = (int32_t)(std::max_element(logits, logits + S.n_vocab) - logits);
    } else {
        const float * h = llama_get_embeddings_layer_inp(S.ctx, S.cfg.split);
        if (!h) throw std::runtime_error("no layer_inp captured -- did you forget --split on this node?");
        res.hidden.assign(h, h + (size_t) n_tokens * S.n_embd);
    }
    return res;
}

static json handle_step(const json & body) {
    std::string request_id = body.value("request_id", std::string("default"));

    std::vector<int32_t> input_ids;
    std::vector<float> hidden_in;
    int32_t n_tokens = 0;

    if (is_first_node()) {
        if (!body.contains("input_ids")) throw std::runtime_error("first node requires input_ids");
        input_ids = body.at("input_ids").get<std::vector<int32_t>>();
        n_tokens = (int32_t) input_ids.size();
    } else {
        if (!body.contains("hidden_states_b64")) throw std::runtime_error("non-first node requires hidden_states_b64");
        auto bytes = b64_decode(body.at("hidden_states_b64").get<std::string>());
        hidden_in = bytes_to_floats(bytes);
        n_tokens = (int32_t)(hidden_in.size() / S.n_embd);
    }

    StepResult r = run_step(is_first_node() ? &input_ids : nullptr, is_first_node() ? nullptr : &hidden_in, n_tokens);

    json resp;
    resp["request_id"] = request_id;
    resp["seq_len"] = n_tokens;
    resp["hidden_size"] = S.n_embd;
    resp["is_last_node"] = S.cfg.is_last_node;

    if (S.cfg.is_last_node) {
        resp["next_token_id"] = r.next_token;
        char piece[256];
        int n = llama_token_to_piece(S.vocab, r.next_token, piece, sizeof(piece), 0, true);
        resp["next_token_text"] = n > 0 ? std::string(piece, n) : std::string();
    } else {
        auto bytes = floats_to_bytes(r.hidden);
        resp["hidden_states_b64"] = b64_encode(bytes.data(), bytes.size());
    }
    return resp;
}

int main(int argc, char ** argv) {
    S.cfg.model_path = "";
    for (int i = 1; i < argc; ++i) {
        std::string a = argv[i];
        if (a == "--model" && i+1 < argc) S.cfg.model_path = argv[++i];
        else if (a == "--layer-start" && i+1 < argc) S.cfg.layer_start = std::atoi(argv[++i]);
        else if (a == "--port" && i+1 < argc) S.cfg.port = std::atoi(argv[++i]);
        else if (a == "--split" && i+1 < argc) S.cfg.split = std::atoi(argv[++i]);
        else if (a == "--layer-end" && i+1 < argc) S.cfg.split = std::atoi(argv[++i]); // alias, matches scripts/pipeline_server.py CLI
        else if (a == "--last" || a == "--is-last-node") S.cfg.is_last_node = true;
        else if (a == "--is-first-node") { S.cfg.is_first_node_flag = true; S.cfg.is_first_node_flag_set = true; }
        else if (a == "--device" || a == "--dtype") { ++i; } // accepted+ignored for CLI parity with pipeline_server.py (CPU/F32 only here)
    }
    if (S.cfg.model_path.empty()) { fprintf(stderr, "usage: %s --model PATH [--layer-start N] [--last] [--port P]\n", argv[0]); return 1; }

    llama_model_params mparams = llama_model_default_params();
    mparams.n_gpu_layers = 0;
    S.model = llama_model_load_from_file(S.cfg.model_path.c_str(), mparams);
    if (!S.model) { fprintf(stderr, "model load failed\n"); return 1; }

    S.vocab = llama_model_get_vocab(S.model);
    S.n_vocab = llama_vocab_n_tokens(S.vocab);
    S.n_embd  = llama_model_n_embd(S.model);
    S.n_layer = llama_model_n_layer(S.model);

    llama_context_params cparams = llama_context_default_params();
    cparams.n_ctx = 4096; cparams.n_batch = 512; cparams.n_ubatch = 512;
    cparams.embeddings = true; // needed so llama_get_embeddings_layer_inp works on non-last nodes
    S.ctx = llama_init_from_model(S.model, cparams);
    if (!S.ctx) { fprintf(stderr, "context init failed\n"); return 1; }

    if (S.cfg.layer_start > 0) {
        llama_set_pipeline_layer_start(S.ctx, S.cfg.layer_start);
    }
    if (!S.cfg.is_last_node) {
        // tap the hidden state feeding the NEXT layer after our cut point
        llama_set_embeddings_layer_inp(S.ctx, S.cfg.split, true);
        // EARLY-EXIT (fixed this session, see README section 5): stop building the
        // graph after our last owned layer instead of recomputing the full model
        // every step. Without this, node0/middle nodes were numerically correct
        // but wasted CPU recomputing norm+lm_head+later layers they don't own.
        llama_set_pipeline_layer_end(S.ctx, S.cfg.split);
    }

    httplib::Server svr;

    svr.Get("/status", [](const httplib::Request &, httplib::Response & res) {
        json j;
        j["model_id"] = S.cfg.model_path;
        j["layer_start"] = S.cfg.layer_start;
        j["is_first_node"] = is_first_node();
        j["is_last_node"] = S.cfg.is_last_node;
        j["total_layers"] = S.n_layer;
        j["layer_end"] = S.cfg.is_last_node ? S.n_layer : S.cfg.split;
        j["active_requests"] = S.active_requests;
        j["device"] = "cpu";
        j["eos_token_id"] = llama_vocab_eos(S.vocab);
        res.set_content(j.dump(), "application/json");
    });

    auto step_handler = [](const httplib::Request & req, httplib::Response & res) {
        try {
            std::lock_guard<std::mutex> lock(S.mu);
            json body = json::parse(req.body);
            json resp = handle_step(body);
            res.set_content(resp.dump(), "application/json");
        } catch (const std::exception & e) {
            res.status = 500;
            res.set_content(std::string("{\"error\":\"") + e.what() + "\"}", "application/json");
        }
    };
    svr.Post("/prefill", step_handler);
    svr.Post("/decode", step_handler);

    svr.Post("/clear", [](const httplib::Request & req, httplib::Response & res) {
        std::lock_guard<std::mutex> lock(S.mu);
        S.next_pos = 0;
        // best-effort: nothing else to free in this single-sequence PoC
        res.set_content("{}", "application/json");
    });

    svr.Post("/tokenize", [](const httplib::Request & req, httplib::Response & res) {
        try {
            json body = json::parse(req.body);
            std::string text;
            if (body.contains("text")) {
                text = body.at("text").get<std::string>();
            } else if (body.contains("messages")) {
                std::vector<llama_chat_message> msgs;
                std::vector<std::string> roles, contents;
                for (auto & m : body.at("messages")) {
                    roles.push_back(m.value("role", "user"));
                    contents.push_back(m.value("content", ""));
                }
                for (size_t i = 0; i < roles.size(); ++i) msgs.push_back({roles[i].c_str(), contents[i].c_str()});
                std::vector<char> buf(8192);
                int32_t n = llama_chat_apply_template(nullptr, msgs.data(), msgs.size(), true, buf.data(), (int32_t)buf.size());
                if (n > (int32_t)buf.size()) { buf.resize(n); n = llama_chat_apply_template(nullptr, msgs.data(), msgs.size(), true, buf.data(), (int32_t)buf.size()); }
                text.assign(buf.data(), n > 0 ? n : 0);
            }
            std::vector<llama_token> toks(text.size() + 8);
            int32_t n = llama_tokenize(S.vocab, text.c_str(), (int32_t)text.size(), toks.data(), (int32_t)toks.size(), true, true);
            if (n < 0) { toks.resize(-n); n = llama_tokenize(S.vocab, text.c_str(), (int32_t)text.size(), toks.data(), (int32_t)toks.size(), true, true); }
            toks.resize(n);
            json ids = json::array();
            for (auto t : toks) ids.push_back((int32_t) t);
            json resp; resp["input_ids"] = ids;
            res.set_content(resp.dump(), "application/json");
        } catch (const std::exception & e) {
            res.status = 500; res.set_content(std::string("{\"error\":\"") + e.what() + "\"}", "application/json");
        }
    });

    svr.Post("/detokenize", [](const httplib::Request & req, httplib::Response & res) {
        try {
            json body = json::parse(req.body);
            std::string text;
            char piece[256];
            for (auto & t : body.at("token_ids")) {
                int n = llama_token_to_piece(S.vocab, t.get<int32_t>(), piece, sizeof(piece), 0, true);
                if (n > 0) text.append(piece, n);
            }
            json resp; resp["text"] = text;
            res.set_content(resp.dump(), "application/json");
        } catch (const std::exception & e) {
            res.status = 500; res.set_content(std::string("{\"error\":\"") + e.what() + "\"}", "application/json");
        }
    });

    fprintf(stderr, "pipeline_server: model=%s layer_start=%d is_last=%d listening on :%d\n",
        S.cfg.model_path.c_str(), S.cfg.layer_start, S.cfg.is_last_node, S.cfg.port);
    svr.listen("127.0.0.1", S.cfg.port);
    return 0;
}
