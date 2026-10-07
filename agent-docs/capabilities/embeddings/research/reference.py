"""Reference vectors for crates/core/src/embeddings's real-model test.

Embeds each text alone with Python's onnxruntime and tokenizers (Hugging
Face's own truncation), so the Rust side's batching, padding, truncation
and pooling are checked against an independent path. Usage:

    pip install onnxruntime tokenizers numpy
    python reference.py <dir>   # <dir> holds bge/ and e5/, each model.onnx + tokenizer.json
    CHAIN_TEST_EMBEDDING_MODELS=<dir> cargo test -p chain-core --lib embeddings
"""

import json
import sys
from pathlib import Path

import numpy as np
import onnxruntime as ort
from tokenizers import Tokenizer

LONG = " ".join(["Attackers are driven by money, ideology, curiosity or revenge."] * 80)
CASES = [
    {
        "model": "bge",
        "pooling": "cls",
        "as": "passage",
        "queryPrefix": None,
        "passagePrefix": None,
        "texts": [
            "Motives: what drives an attacker",
            "why do people hack",
            "Photosynthesis turns light into chemical energy.",
            LONG,
            "",
        ],
    },
    {
        "model": "e5",
        "pooling": "mean",
        "as": "query",
        "queryPrefix": "query: ",
        "passagePrefix": "passage: ",
        "texts": [
            "why do people hack",
            "pourquoi les gens piratent-ils des ordinateurs",
            "人们为什么会入侵电脑",
            LONG,
            "Motives: what drives an attacker",
        ],
    },
]


def embed(session, tokenizer, text, pooling):
    encoding = tokenizer.encode(text)
    ids = np.array([encoding.ids], dtype=np.int64)
    feeds = {}
    for i in session.get_inputs():
        if "input_ids" in i.name:
            feeds[i.name] = ids
        elif "attention_mask" in i.name:
            feeds[i.name] = np.ones_like(ids)
        elif "token_type_ids" in i.name:
            feeds[i.name] = np.zeros_like(ids)
    hidden = session.run(None, feeds)[0][0]
    vector = hidden[0] if pooling == "cls" else hidden.mean(axis=0)
    return (vector / np.linalg.norm(vector)).tolist()


def main(root):
    out = []
    for case in CASES:
        folder = root / case["model"]
        session = ort.InferenceSession(str(folder / "model.onnx"))
        full = Tokenizer.from_file(str(folder / "tokenizer.json"))
        full.no_truncation()
        full.no_padding()
        cut = Tokenizer.from_file(str(folder / "tokenizer.json"))
        cut.enable_truncation(512)
        cut.no_padding()
        prefix = case["queryPrefix"] if case["as"] == "query" else case["passagePrefix"]
        texts = [(prefix or "") + t for t in case["texts"]]
        out.append({
            **case,
            "tokens": [len(full.encode(t).ids) for t in texts],
            "vectors": [embed(session, cut, t, case["pooling"]) for t in texts],
        })
    (root / "reference.json").write_text(json.dumps(out))


if __name__ == "__main__":
    main(Path(sys.argv[1]))
