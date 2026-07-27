# Génère un GGUF Gemma3 "tiny random" à 3 couches à partir du modèle de test à 2 couches
# (zelk12/gemma-3-tiny-random-Q6_K-GGUF), en copiant TOUS les tenseurs/metadonnées tels
# quels (y compris les Q8_0 déjà quantifiés, sans les déquantifier) et en dupliquant les
# tenseurs de la couche 1 pour créer une couche 2 -- les poids sont aléatoires de toute
# façon, seule la forme/le dtype comptent pour ce test de plomberie (vérifier qu'un vrai
# noeud du milieu, pipeline_layer_start>0 ET pipeline_layer_end<n_layer simultanément,
# fonctionne). Ne nécessite ni torch ni transformers -- juste `pip install gguf` (numpy).
#
# Usage : python3 build_3layer_test_gguf.py
# (chemins SRC/DST en dur ci-dessous, adapter si besoin)

import numpy as np
from gguf.gguf_reader import GGUFReader
from gguf.gguf_writer import GGUFWriter
from gguf.constants import GGUFValueType, RopeScalingType

SRC = "/tmp/models/gemma3-tiny.gguf"
DST = "/tmp/models/gemma3-tiny-3l.gguf"

r = GGUFReader(SRC)

def scalar(key):
    f = r.fields[key]
    v = f.parts[f.data[0]]
    if f.types[-1] == GGUFValueType.STRING:
        return bytes(v).decode('utf-8', 'replace')
    return v[0].item()

print("Extracting tokenizer arrays...")
tok_f = r.fields['tokenizer.ggml.tokens']
tokens = [bytes(tok_f.parts[i]).decode('utf-8', 'replace') for i in tok_f.data]
score_f = r.fields['tokenizer.ggml.scores']
scores = [float(score_f.parts[i][0]) for i in score_f.data]
type_f = r.fields['tokenizer.ggml.token_type']
ttypes = [int(type_f.parts[i][0]) for i in type_f.data]
print(f"  {len(tokens)} tokens extracted")

chat_template = scalar('tokenizer.chat_template')

w = GGUFWriter(DST, arch="gemma3")

w.add_name("Gemma 3 Tiny Random 3-Layer (test)")
w.add_size_label("8.4M-3L")
w.add_context_length(int(scalar('gemma3.context_length')))
w.add_embedding_length(int(scalar('gemma3.embedding_length')))
w.add_block_count(3)  # <-- le seul changement qui compte : 2 -> 3 couches
w.add_feed_forward_length(int(scalar('gemma3.feed_forward_length')))
w.add_head_count(int(scalar('gemma3.attention.head_count')))
w.add_head_count_kv(int(scalar('gemma3.attention.head_count_kv')))
w.add_layer_norm_rms_eps(float(scalar('gemma3.attention.layer_norm_rms_epsilon')))
w.add_key_length(int(scalar('gemma3.attention.key_length')))
w.add_value_length(int(scalar('gemma3.attention.value_length')))
w.add_rope_freq_base(float(scalar('gemma3.rope.freq_base')))
w.add_sliding_window(int(scalar('gemma3.attention.sliding_window')))
w.add_rope_scaling_type(RopeScalingType(scalar('gemma3.rope.scaling.type')))
w.add_rope_scaling_factor(float(scalar('gemma3.rope.scaling.factor')))
w.add_file_type(int(scalar('general.file_type')))
w.add_quantization_version(int(scalar('general.quantization_version')))

w.add_tokenizer_model(scalar('tokenizer.ggml.model'))
w.add_tokenizer_pre(scalar('tokenizer.ggml.pre'))
w.add_token_list(tokens)
w.add_token_scores(scores)
w.add_token_types(ttypes)
w.add_bos_token_id(int(scalar('tokenizer.ggml.bos_token_id')))
w.add_eos_token_id(int(scalar('tokenizer.ggml.eos_token_id')))
w.add_unk_token_id(int(scalar('tokenizer.ggml.unknown_token_id')))
w.add_pad_token_id(int(scalar('tokenizer.ggml.padding_token_id')))
w.add_add_bos_token(bool(scalar('tokenizer.ggml.add_bos_token')))
w.add_add_sep_token(bool(scalar('tokenizer.ggml.add_sep_token')))
w.add_add_eos_token(bool(scalar('tokenizer.ggml.add_eos_token')))
w.add_add_space_prefix(bool(scalar('tokenizer.ggml.add_space_prefix')))
w.add_chat_template(chat_template)

tensors = {t.name: t for t in r.tensors}

def copy_tensor(dst_name, src_tensor):
    # Copie les octets bruts tels quels (Q8_0 quantifié ou F32), sans déquantifier --
    # raw_shape doit être la "byte shape" (t.data.shape), pas la forme logique (ne),
    # sinon gguf-py tente de la reconvertir et échoue sur les tenseurs quantifiés.
    w.add_tensor(dst_name, src_tensor.data, raw_shape=list(src_tensor.data.shape), raw_dtype=src_tensor.tensor_type)

# tenseurs globaux, forme inchangée quel que soit block_count
copy_tensor("token_embd.weight", tensors["token_embd.weight"])
copy_tensor("output_norm.weight", tensors["output_norm.weight"])

LAYER_SUFFIXES = ["attn_k.weight","attn_k_norm.weight","attn_norm.weight","attn_output.weight",
                   "attn_q.weight","attn_q_norm.weight","attn_v.weight","ffn_down.weight",
                   "ffn_gate.weight","ffn_norm.weight","ffn_up.weight",
                   "post_attention_norm.weight","post_ffw_norm.weight"]

# couches 0 et 1 : copiées telles quelles depuis la source
for il in (0, 1):
    for suffix in LAYER_SUFFIXES:
        name = f"blk.{il}.{suffix}"
        copy_tensor(name, tensors[name])

# couche 2 (NOUVELLE) : duplique les tenseurs de la couche 1 (poids aléatoires,
# seule la forme/le dtype comptent pour ce test)
for suffix in LAYER_SUFFIXES:
    copy_tensor(f"blk.2.{suffix}", tensors[f"blk.1.{suffix}"])

print("Writing header/kv/tensor-info/data...")
w.write_header_to_file()
w.write_kv_data_to_file()
w.write_tensors_to_file(progress=False)
w.close()
print("Done:", DST)
