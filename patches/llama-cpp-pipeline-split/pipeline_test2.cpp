// Pipeline-split correctness test v2: 2 autoregressive steps (prefill + 1 decode)
// across a real 2-node split, using ONLY the proven mechanism from v1:
//   - node0: full-model context (both layers computed), taps the hidden state
//            feeding layer 1 via llama_set_embeddings_layer_inp + llama_get_embeddings_layer_inp
//   - node1: pipeline_layer_start=1, receives that hidden state via llama_batch.embd
// This checks whether per-context LOCAL KV-cache (no cross-node transfer) is enough
// for correct multi-step generation across the split.
//
// (An earlier attempt also added a "stop early" pipeline_layer_end on node0, to avoid
// wasting compute on layer 1 there -- that combination crashed inside ggml's scheduler
// (GGML_ASSERT(buffer) in ggml-backend.cpp) when the graph's output tensor set changes
// between reserve() calls on the same context. Reverted; not part of the shipped patch.
// node0 here still computes the full model every step -- correct, just not maximally
// efficient. See README "known limitations".)

#include "llama.h"
#include "llama-ext.h"
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <vector>
#include <cmath>
#include <algorithm>

static void fatal(const char * msg) { fprintf(stderr, "FATAL: %s\n", msg); exit(1); }
static int argmax(const float * v, int n) { return (int)(std::max_element(v, v + n) - v); }

static llama_context * make_ctx(llama_model * model) {
    llama_context_params p = llama_context_default_params();
    p.n_ctx = 64; p.n_batch = 64; p.n_ubatch = 64; p.no_perf = true;
    llama_context * ctx = llama_init_from_model(model, p);
    if (!ctx) fatal("ctx init failed");
    return ctx;
}

int main() {
    llama_model_params mparams = llama_model_default_params();
    mparams.n_gpu_layers = 0;
    llama_model * model = llama_model_load_from_file("/tmp/models/gemma3-tiny.gguf", mparams);
    if (!model) fatal("model load failed");

    const llama_vocab * vocab = llama_model_get_vocab(model);
    const int32_t n_vocab = llama_vocab_n_tokens(vocab);
    const int32_t n_embd  = llama_model_n_embd(model);
    printf("n_vocab=%d n_embd=%d\n", n_vocab, n_embd);

    std::vector<llama_token> prompt = {2, 55123, 9821, 174, 61000, 3};
    for (auto & t : prompt) if (t >= n_vocab) t = t % n_vocab;
    const int32_t n_prompt = (int32_t) prompt.size();

    // ================= BASELINE: single context, full model, 2 autoregressive steps =================
    llama_context * ctxBase = make_ctx(model);
    {
        llama_batch b = llama_batch_init(n_prompt, 0, 1);
        b.n_tokens = n_prompt;
        for (int i = 0; i < n_prompt; ++i) { b.token[i] = prompt[i]; b.pos[i] = i; b.n_seq_id[i] = 1; b.seq_id[i][0] = 0; b.logits[i] = (i == n_prompt-1); }
        if (llama_decode(ctxBase, b) != 0) fatal("baseline prefill failed");
        llama_batch_free(b);
    }
    const float * lb1 = llama_get_logits_ith(ctxBase, n_prompt - 1);
    std::vector<float> logits_base_step1(lb1, lb1 + n_vocab);
    int tok_base_1 = argmax(logits_base_step1.data(), n_vocab);

    {
        llama_batch b = llama_batch_init(1, 0, 1);
        b.n_tokens = 1;
        b.token[0] = tok_base_1; b.pos[0] = n_prompt; b.n_seq_id[0] = 1; b.seq_id[0][0] = 0; b.logits[0] = 1;
        if (llama_decode(ctxBase, b) != 0) fatal("baseline decode failed");
        llama_batch_free(b);
    }
    const float * lb2 = llama_get_logits_ith(ctxBase, 0);
    std::vector<float> logits_base_step2(lb2, lb2 + n_vocab);
    int tok_base_2 = argmax(logits_base_step2.data(), n_vocab);

    printf("BASELINE: step1 token=%d step2 token=%d\n", tok_base_1, tok_base_2);

    // ================= SPLIT: node0 (full model, taps layer-1 input) + node1 (pipeline_layer_start=1) ==
    llama_context * ctx0 = make_ctx(model);
    llama_set_embeddings_layer_inp(ctx0, 1, true);

    llama_context * ctx1 = make_ctx(model);
    llama_set_pipeline_layer_start(ctx1, 1);

    auto run_node0 = [&](const std::vector<llama_token> & toks, int pos0) -> std::vector<float> {
        int n = (int) toks.size();
        llama_batch b = llama_batch_init(n, 0, 1);
        b.n_tokens = n;
        for (int i = 0; i < n; ++i) { b.token[i] = toks[i]; b.pos[i] = pos0 + i; b.n_seq_id[i] = 1; b.seq_id[i][0] = 0; b.logits[i] = 1; }
        if (llama_decode(ctx0, b) != 0) fatal("node0 decode failed");
        llama_batch_free(b);
        const float * h = llama_get_embeddings_layer_inp(ctx0, 1);
        if (!h) fatal("node0: no layer_inp(1) captured");
        return std::vector<float>(h, h + (size_t) n * n_embd);
    };

    auto run_node1 = [&](const std::vector<float> & h, int n, int pos0) -> std::vector<float> {
        llama_batch b = llama_batch_init(n, n_embd, 1);
        b.n_tokens = n;
        memcpy(b.embd, h.data(), sizeof(float) * n * n_embd);
        for (int i = 0; i < n; ++i) { b.pos[i] = pos0 + i; b.n_seq_id[i] = 1; b.seq_id[i][0] = 0; b.logits[i] = 1; }
        if (llama_decode(ctx1, b) != 0) fatal("node1 decode failed");
        llama_batch_free(b);
        const float * l = llama_get_logits_ith(ctx1, n - 1);
        if (!l) fatal("node1: no logits");
        return std::vector<float>(l, l + n_vocab);
    };

    // step 1: prefill across the split
    auto h1 = run_node0(prompt, 0);
    auto logits_split_step1 = run_node1(h1, n_prompt, 0);
    int tok_split_1 = argmax(logits_split_step1.data(), n_vocab);

    // step 2: decode across the split -- each node's KV-cache is purely LOCAL and persists
    // automatically between llama_decode() calls on the SAME context; no cross-node KV
    // transfer is used or needed here, only the residual-stream hidden state crosses nodes.
    auto h2 = run_node0({tok_split_1}, n_prompt);
    auto logits_split_step2 = run_node1(h2, 1, n_prompt);
    int tok_split_2 = argmax(logits_split_step2.data(), n_vocab);

    printf("SPLIT   : step1 token=%d step2 token=%d\n", tok_split_1, tok_split_2);

    auto diff_stats = [&](const std::vector<float> & a, const std::vector<float> & b) {
        double mx = 0, ss = 0, sb = 0;
        for (size_t i = 0; i < a.size(); ++i) {
            double d = (double)a[i] - (double)b[i];
            mx = std::max(mx, std::fabs(d));
            ss += d*d; sb += (double)a[i]*a[i];
        }
        return std::pair<double,double>(mx, std::sqrt(ss)/(std::sqrt(sb)+1e-12));
    };

    auto r1 = diff_stats(logits_base_step1, logits_split_step1);
    auto r2 = diff_stats(logits_base_step2, logits_split_step2);

    printf("\n=== RESULT ===\n");
    printf("step1: token match=%s max_abs_diff=%g rel_l2=%g\n", tok_base_1==tok_split_1?"YES":"NO", r1.first, r1.second);
    printf("step2: token match=%s max_abs_diff=%g rel_l2=%g\n", tok_base_2==tok_split_2?"YES":"NO", r2.first, r2.second);

    bool ok = (tok_base_1==tok_split_1) && (tok_base_2==tok_split_2) && r1.first < 1e-2 && r2.first < 1e-2;
    printf("OVERALL: %s\n", ok ? "PASS" : "FAIL");
    return ok ? 0 : 2;
}
