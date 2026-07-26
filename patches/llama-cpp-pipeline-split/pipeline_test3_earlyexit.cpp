// Repro/regression harness for the pipeline_layer_end early-exit crash
// (see README.md "Palier G — pipeline_layer_end: cause racine trouvée et
// corrigée"). node0 uses BOTH pipeline_layer_start=0 (first node) AND
// pipeline_layer_end=1 (early exit, skip norm+lm_head) on a 2-layer model,
// tapping embeddings_layer_inp(1) to get the hidden state. node1 uses
// pipeline_layer_start=1 (normal, to n_layer). 2 autoregressive steps
// (prefill + 1 decode), same protocol as pipeline_test2.cpp.
//
// Before the fix (build_inp_out_ids() called unconditionally before the
// early-exit branch): GGML_ASSERT(buffer) failed in
// ggml_backend_buffer_get_type (ggml-backend.cpp), because the unused
// inp_out_ids tensor never gets allocated a buffer when the graph early-exits
// before the code that consumes it.
//
// After the fix (0001-pipeline-split-poc.patch, current version): this test
// passes with 0 diff vs pipeline_test2.cpp's baseline (token 34658 on both
// steps, no crash).
#include "llama.h"
#include "llama-ext.h"
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <vector>
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

    // node0: first node, EARLY EXIT after layer 0 (pipeline_layer_end=1)
    llama_context * ctx0 = make_ctx(model);
    llama_set_embeddings_layer_inp(ctx0, 1, true);
    llama_set_pipeline_layer_end(ctx0, 1);   // <-- the fixed feature

    // node1: normal continuation from layer 1
    llama_context * ctx1 = make_ctx(model);
    llama_set_pipeline_layer_start(ctx1, 1);

    auto run_node0 = [&](const std::vector<llama_token> & toks, int pos0) -> std::vector<float> {
        int n = (int) toks.size();
        printf("  [node0] decode n_tokens=%d pos0=%d\n", n, pos0); fflush(stdout);
        llama_batch b = llama_batch_init(n, 0, 1);
        b.n_tokens = n;
        for (int i = 0; i < n; ++i) { b.token[i] = toks[i]; b.pos[i] = pos0 + i; b.n_seq_id[i] = 1; b.seq_id[i][0] = 0; b.logits[i] = 1; }
        int rc = llama_decode(ctx0, b);
        printf("  [node0] decode rc=%d\n", rc); fflush(stdout);
        if (rc != 0) fatal("node0 decode failed");
        llama_batch_free(b);
        const float * h = llama_get_embeddings_layer_inp(ctx0, 1);
        if (!h) fatal("node0: no layer_inp(1) captured");
        return std::vector<float>(h, h + (size_t) n * n_embd);
    };

    auto run_node1 = [&](const std::vector<float> & h, int n, int pos0) -> std::vector<float> {
        printf("  [node1] decode n_tokens=%d pos0=%d\n", n, pos0); fflush(stdout);
        llama_batch b = llama_batch_init(n, n_embd, 1);
        b.n_tokens = n;
        memcpy(b.embd, h.data(), sizeof(float) * n * n_embd);
        for (int i = 0; i < n; ++i) { b.pos[i] = pos0 + i; b.n_seq_id[i] = 1; b.seq_id[i][0] = 0; b.logits[i] = 1; }
        int rc = llama_decode(ctx1, b);
        printf("  [node1] decode rc=%d\n", rc); fflush(stdout);
        if (rc != 0) fatal("node1 decode failed");
        llama_batch_free(b);
        const float * l = llama_get_logits_ith(ctx1, n - 1);
        if (!l) fatal("node1: no logits");
        return std::vector<float>(l, l + n_vocab);
    };

    printf("=== step 1: prefill ===\n"); fflush(stdout);
    auto h1 = run_node0(prompt, 0);
    auto logits1 = run_node1(h1, n_prompt, 0);
    int tok1 = argmax(logits1.data(), n_vocab);
    printf("step1 token=%d\n", tok1);

    printf("=== step 2: decode ===\n"); fflush(stdout);
    auto h2 = run_node0({tok1}, n_prompt);
    auto logits2 = run_node1(h2, 1, n_prompt);
    int tok2 = argmax(logits2.data(), n_vocab);
    printf("step2 token=%d\n", tok2);

    printf("OVERALL: PASS (no crash) tokens=%d,%d\n", tok1, tok2);
    return 0;
}
