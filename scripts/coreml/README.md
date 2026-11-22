# Native CoreML developer tools

The installed app needs neither Python nor these tools. It reconstructs learned weights from verified, pinned ONNX files using data-free templates, then compiles a private local cache. Never commit weighted packages or compiled caches.

Use a separate macOS Python environment with `requirements.txt`. These developer dependencies are not application dependencies. The exporter supports the pinned EmbeddingGemma 2 graphs only; it is not a general ONNX converter.

```sh
python export_models.py --models /absolute/models --output /absolute/private-output --templates
python export_models.py --models /absolute/models --precision q8 --bucket 512 --output /absolute/private-package
python benchmark.py compare candidate.bin reference.bin
python benchmark.py measure result.json -- /absolute/benchmark arguments
```

`--templates` checks source graph checksums for all three precisions and Q8 weight checksums, exports all backbone buckets in bounded workers, and externalizes every learned constant (including inline learned scalars). Vision templates are copied from the versioned, data-free native resource directory. Runtime reconstruction verifies the complete selected source precision. Template graph protobufs may serialize maps in different orders; compare parsed messages, not raw file hashes, when reproducing an export.

`benchmark.swift` is an optional isolated native predictor. Compile it with `xcrun swiftc -O -target arm64-apple-macos15.0 benchmark.swift -o /absolute/private-benchmark`. The Python harness records the actual child process's RSS and Darwin footprint, aborting when either exceeds 16 GiB. Footprint is an accounting ledger and is not total physical system RAM. Binary comparisons require finite, nonzero 768-dimensional vectors and cosine similarity strictly greater than 0.9999; the threshold is not configurable.

The source is [onnx-community/embeddinggemma-2-ONNX](https://huggingface.co/onnx-community/embeddinggemma-2-ONNX), revision `da4d2517b6a48d3de6d480ac2126ef5353c30713`. Model terms remain those of the upstream model; this repository's software license does not relicense weights. See [third-party notices](../../THIRD_PARTY_NOTICES.md) and [inference notes](../../docs/inference.md).
