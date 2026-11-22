# Inference

EmbeddingGemma 2 produces one normalized 768-dimensional vector per photo or whole video. Q8 is the default source model; Q4 and FP32 are optional. Videos must have a known duration of at most 32 seconds. Queries support up to 8192 model tokens.

## Backends

- Apple Silicon/macOS 15+: native CoreML with CPU-and-GPU placement.
- Native initialization or execution failure: retry on ONNX Metal; Metal initialization failure: use CPU.
- Intel Mac and Windows: ONNX CPU.

Model management reports the actual backend. Cancellation and invalid inputs do not trigger retries. Backend changes retain model identity; precision changes rebuild the index.

The native adapter uses sparse CPU embedding lookup, shared-weight context buckets, FP32 positions and sensitive reductions, and mixed-precision GPU matrices. Selecting FP32 chooses FP32 source weights, not exclusively FP32 arithmetic. Verified source files remain unchanged; local compiled caches are checked before reuse. Installed apps need no Python or conversion tools. See [developer tools](../scripts/coreml/README.md) for regeneration.

## Validation and limits

Q8, Q4, and FP32 passed cosine similarity **>0.9999** against their original CPU outputs for short and long English/Chinese text, varied image sizes, and complete 32-second static and changing-frame videos. Tests also cover cancellation, oversized inputs, cache reuse, and native-to-Metal recovery. See [testing](testing.md) to reproduce.

Local Q8 measurements on Apple M6/macOS 27, using the production encoder with three requests per operation and the first excluded:

| Operation                               | Metal warm | CoreML warm |
| --------------------------------------- | ---------: | ----------: |
| Photo                                   | 743–745 ms |      273 ms |
| Complete 32-second changing-frame video |    10.38 s | 4.76–4.77 s |

In the mixed query/photo/video run, peak Darwin footprint was 4.97 GiB on Metal and 1.08 GiB on CoreML; peak RSS was 1.82 and 2.00 GiB respectively. Footprint is an accounting ledger, not total physical RAM. First requests and cache preparation can be slower, and results vary by hardware. No ANE acceleration or Intel/Windows performance is claimed.
