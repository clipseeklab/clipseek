# Data-free CoreML resources

These graphs and byte-span layouts describe the pinned EmbeddingGemma 2 ONNX model. They contain graph structure and reconstruction recipes, not learned tensor payloads. The backbone includes no inline learned scalars. Layout headers contain storage metadata, never tensor data.

At runtime, selected Q8/Q4/FP32 source files are verified before weights are reconstructed locally. All backbone buckets share a weight file. The selected source embedding table is accessed sparsely on CPU; positional calculations and sensitive reductions remain FP32. Templates use mixed-precision GPU matrices and fused attention, preserving the strict output acceptance criterion documented in [inference notes](../../../docs/inference.md).

Developer regeneration instructions are in [developer tools](../../../scripts/coreml/README.md). Do not add weighted `.mlpackage` exports, compiled `.mlmodelc` directories, or any `weight.bin` here. Upstream model licensing is described in [third-party notices](../../../THIRD_PARTY_NOTICES.md).
