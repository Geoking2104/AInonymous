// Pipeline-split correctness test (PoC, Gemma3 dense, CPU-only).
//
// Loads a tiny random Gemma3 GGUF (2 layers), then compares:
//   (A) baseline: full model, single context, layers [0,2) -> logits_baseline
//   (B) split:    ctx0 computes layer [0,1), we extract t_layer_inp[1]
//                 (hidden state fed INTO layer 1) via llama_get_embeddings_layer_inp,
//                 then a FRESH ctx1 with pipeline_layer_start=1 is fed that hidden
//                 state as llama_batch.embd and computes layer [1,2) -> logits_split
//
// Success = argmax(logits_baseline) == argmax(logits_split) AND max abs diff is tiny.

#include "llama.h"
#include "llama-ext.h"
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <vector>
#include <cmath>
#include <algorithm>

static void fatal(const char * msg) {
    fprintf(stderr, "FATAL: %s\n", msg);
    exit(1);
}

int main() {
    const char * model_path = "/tmp/models/gemma3-tiny.gguf";

    llama_model_params mparams = llama_model_default_params();
    mparams.n_gpu_layers = 0;

    llama_model * model = llama_model_load_from_file(model_path, mparams);
    if (!model) fatal("model load failed");

    const llama_vocab * vocab = llama_model_get_vocab(model);
    const int32_t n_vocab = llama_vocab_n_tokens(vocab);
    const int32_t n_embd  = llama_model_n_embd(model);
    printf("n_vocab=%d n_embd=%d\n", n_vocab, n_embd);

    // fixed prompt: bos + a few arbitrary valid token ids
    std::vector<llama_token> prompt = {2, 55123, 9821, 174, 61000, 3};
    for (auto & t : prompt) if (t >= n_vocab) t = t % n_vocab;
    const int32_t n_tokens = (int32_t) prompt.size();

    // ---------- context A: baseline, full model ----------
    llama_context_params cparamsA = llama_context_default_params();
    cparamsA.n_ctx   = 64;
    cparamsA.n_batch = 64;
    cparamsA.n_ubatch = 64;
    cparamsA.no_perf = true;

    llama_context * ctxA = llama_init_from_model(model, cparamsA);
    if (!ctxA) fatal("ctxA init failed");

    // ask to also tap the hidden state that feeds layer 1 (i.e. output of layer 0)
    llama_set_embeddings_layer_inp(ctxA, 1, true);

    llama_batch batchA = llama_batch_init(n_tokens, 0, 1);
    batchA.n_tokens = n_tokens;
    for (int i = 0; i < n_tokens; ++i) {
        batchA.token[i]      = prompt[i];
        batchA.pos[i]        = i;
        batchA.n_seq_id[i]   = 1;
        batchA.seq_id[i][0]  = 0;
        batchA.logits[i]     = 1; // request logits for every position (simplifies comparison)
    }

    if (llama_decode(ctxA, batchA) != 0) fatal("ctxA decode failed");

    const float * logits_baseline_last = llama_get_logits_ith(ctxA, n_tokens - 1);
    if (!logits_baseline_last) fatal("no baseline logits");
    std::vector<float> logits_baseline(logits_baseline_last, logits_baseline_last + n_vocab);

    const float * h_split_src = llama_get_embeddings_layer_inp(ctxA, 1);
    if (!h_split_src) fatal("no layer_inp(1) captured -- extraction hook not wired?");
    std::vector<float> h_split(h_split_src, h_split_src + (size_t) n_tokens * n_embd);
    {
        double s = 0, ss = 0; float mn = h_split[0], mx = h_split[0];
        for (float v : h_split) { s += v; ss += (double)v*v; mn = std::min(mn, v); mx = std::max(mx, v); }
        double mean = s / h_split.size();
        printf("h_split stats: n=%zu mean=%g std=%g min=%g max=%g\n", h_split.size(), mean, std::sqrt(ss/h_split.size()-mean*mean), mn, mx);
    }
    {
        double s = 0, ss = 0; float mn = logits_baseline[0], mx = logits_baseline[0];
        for (float v : logits_baseline) { s += v; ss += (double)v*v; mn = std::min(mn, v); mx = std::max(mx, v); }
        double mean = s / logits_baseline.size();
        printf("logits_baseline stats: mean=%g std=%g min=%g max=%g\n", mean, std::sqrt(ss/logits_baseline.size()-mean*mean), mn, mx);
    }

    printf("baseline argmax(last) = %d\n",
        (int)(std::max_element(logits_baseline.begin(), logits_baseline.end()) - logits_baseline.begin()));

    // ---------- context B: split, resume from layer 1 ----------
    llama_context_params cparamsB = llama_context_default_params();
    cparamsB.n_ctx    = 64;
    cparamsB.n_batch  = 64;
    cparamsB.n_ubatch = 64;
    cparamsB.no_perf  = true;

    llama_context * ctxB = llama_init_from_model(model, cparamsB);
    if (!ctxB) fatal("ctxB init failed");

    llama_set_pipeline_layer_start(ctxB, 1);

    llama_batch batchB = llama_batch_init(n_tokens, n_embd, 1);
    batchB.n_tokens = n_tokens;
    memcpy(batchB.embd, h_split.data(), sizeof(float) * n_tokens * n_embd);
    for (int i = 0; i < n_tokens; ++i) {
        batchB.pos[i]       = i; // MUST match original absolute positions (RoPE!)
        batchB.n_seq_id[i]  = 1;
        batchB.seq_id[i][0] = 0;
        batchB.logits[i]    = 1;
    }

    if (llama_decode(ctxB, batchB) != 0) fatal("ctxB decode failed");

    const float * logits_split_last = llama_get_logits_ith(ctxB, n_tokens - 1);
    if (!logits_split_last) fatal("no split logits");
    std::vector<float> logits_split(logits_split_last, logits_split_last + n_vocab);

    int argmax_split = (int)(std::max_element(logits_split.begin(), logits_split.end()) - logits_split.begin());
    printf("split    argmax(last) = %d\n", argmax_split);

    // ---------- compare ----------
    double max_abs_diff = 0.0, sum_sq_diff = 0.0, sum_sq_base = 0.0;
    for (int i = 0; i < n_vocab; ++i) {
        double d = (double)logits_baseline[i] - (double)logits_split[i];
        max_abs_diff = std::max(max_abs_diff, std::fabs(d));
        sum_sq_diff += d * d;
        sum_sq_base += (double)logits_baseline[i] * (double)logits_baseline[i];
    }
    double rel_l2 = std::sqrt(sum_sq_diff) / (std::sqrt(sum_sq_base) + 1e-12);

    int argmax_base = (int)(std::max_element(logits_baseline.begin(), logits_baseline.end()) - logits_baseline.begin());

    printf("\n=== RESULT ===\n");
    printf("argmax match      : %s (baseline=%d split=%d)\n", argmax_base == argmax_split ? "YES" : "NO", argmax_base, argmax_split);
    printf("max abs logit diff: %g\n", max_abs_diff);
    printf("relative L2 diff  : %g\n", rel_l2);

    llama_batch_free(batchA);
    llama_batch_free(batchB);
    llama_free(ctxA);
    llama_free(ctxB);
    llama_model_free(model);

    return (argmax_base == argmax_split && rel_l2 < 1e-3) ? 0 : 2;
}
