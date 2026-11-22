"""Externalize every learned constant and share one data-free layout across buckets."""
from pathlib import Path
import json, re, struct, base64, hashlib, shutil
import numpy as np, onnx, coremltools as ct
from onnx import numpy_helper
from coremltools.libmilstoragepython import _BlobStorageReader, _BlobStorageWriter
from _catalogue import recipe_maps, source_model
from _source import load_model

def collect(models_root, stage, output, vision_assets, revision):
    R = Path(stage)
    OUT = Path(output)
    OUT.mkdir(exist_ok=True)
    BUCKETS = [32, 128, 256, 512, 1024, 2048, 2560, 4096, 4608, 8192]
    recipes = recipe_maps(models_root, 'model')
    (original, _) = source_model(models_root, 'fp32', 'model')
    base_nodes = {n.name: n for n in original.graph.node}
    base_names = {t.name for t in original.graph.initializer}
    model = load_model(models_root, 'q8')
    tensors = {t.name: t for t in model.graph.initializer}
    aliases = {key: key for key in tensors if key in base_names}

    def normalized(name):
        return name.removesuffix('_Q4').removesuffix('_quant').removesuffix('_Quant')
    for n in model.graph.node:
        if normalized(n.name) not in base_nodes:
            continue
        b = base_nodes[normalized(n.name)]
        if n.op_type == 'MatMul' and n.input[1].endswith('_dense'):
            aliases[n.input[1]] = b.input[1]
        if n.op_type == 'Gather' and n.input[0].endswith('_dense'):
            aliases[n.input[0]] = b.input[0]
    arrays = {}

    def array(key):
        if key not in arrays:
            arrays[key] = numpy_helper.to_array(tensors[key])
        return arrays[key]
    DT = {10: np.float16, 11: np.float32, 23: np.int32, 32: np.uint16}
    root = R / 'q8-select-512'
    old_file = root / 'text.mlpackage/Data/com.apple.CoreML/weights/weight.bin'
    expected = hashlib.sha256(old_file.read_bytes()).hexdigest()
    reader = _BlobStorageReader(str(old_file))
    temporary = R / 'body-weights.bin'
    writer = _BlobStorageWriter(str(temporary))
    offsets = {}
    scalars = {}
    segments = []

    def shape_of(v):
        return [d.constant.size for d in v.type.tensorType.dimensions]

    def blob(v):
        d = v.type.tensorType.dataType
        o = v.blobFileValue.offset
        a = {10: lambda : reader.read_fp16_data(o).view(np.float16), 11: lambda : reader.read_float_data(o), 23: lambda : reader.read_int32_data(o), 32: lambda : reader.read_uint16_data(o)}[d]()
        return np.asarray(a).reshape(shape_of(v))

    def immediate(v):
        d = v.type.tensorType.dataType
        t = v.immediateValue.tensor
        a = np.frombuffer(t.bytes.values, np.float16) if d == 10 else np.asarray(t.floats.values, np.float32)
        return a.reshape(shape_of(v))

    def recipe(name, arr, names):
        prefix = re.match('^(w\\d+)', name)
        if prefix:
            key = names[prefix[1]]
            canonical = aliases[key]
            a = array(key)
            if np.issubdtype(a.dtype, np.floating):
                a = np.clip(a, -65504, 65504)
            else:
                a = np.clip(a, -2147483648, 2147483647)
            a = a.astype(arr.dtype)
            if not np.array_equal(a.reshape(arr.shape), arr):
                if a.ndim == 2 and np.array_equal(a.T.reshape(arr.shape), arr):
                    raise ValueError('Unexpected transposed learned weight ' + name)
                raise ValueError('Learned weight changed during export ' + name)
            return {'recipes': {p: recipes[p][canonical] for p in recipes}, 'transpose': False, 'tensor': canonical}
        if np.all(arr == arr.flat[0]) and float(arr.flat[0]) in [0, 1, -1, -65504]:
            return {'fill': float(arr.flat[0])}
        if np.array_equal(arr.reshape(-1), np.arange(arr.size, dtype=arr.dtype)):
            return {'range': True}
        raise ValueError('Unmapped non-parameter blob ' + name)

    def put(v, name, names, arr):
        x = arr.flatten().copy()
        d = v.type.tensorType.dataType
        offset = {10: lambda : writer.write_fp16_data(x.view(np.uint16)), 11: lambda : writer.write_float_data(x), 23: lambda : writer.write_int32_data(x), 32: lambda : writer.write_uint16_data(x)}[d]()
        segments.append({'metadataOffset': offset, 'dtype': np.dtype(DT[d]).name, 'count': int(arr.size), 'source': recipe(name, arr, names)})
        return offset
    for bucket in BUCKETS:
        directory = R / ('q8-select-' + str(bucket))
        path = directory / 'text.mlpackage'
        assert hashlib.sha256((path / 'Data/com.apple.CoreML/weights/weight.bin').read_bytes()).hexdigest() == expected, ('Unshared body data', bucket)
        names = json.loads((directory / 'text-sources.json').read_text())
        spec = ct.models.MLModel(str(path), skip_model_load=True).get_spec()
        externalized = 0
        for f in spec.mlProgram.functions.values():
            for b in f.block_specializations.values():
                for op in b.operations:
                    if op.type != 'const':
                        continue
                    value = op.attributes['val']
                    name = op.outputs[0].name
                    is_blob = value.WhichOneof('value') == 'blobFileValue'
                    prefix = re.match('^(w\\d+)', name)
                    if is_blob:
                        old = value.blobFileValue.offset
                        if old not in offsets:
                            offsets[old] = put(value, name, names, blob(value))
                        offset = offsets[old]
                    elif prefix and value.type.tensorType.dataType in [10, 11] and (not aliases[names[prefix[1]]].startswith('/model/constants/')):
                        key = (aliases[names[prefix[1]]], value.type.tensorType.dataType)
                        if key not in scalars:
                            scalars[key] = put(value, name, names, immediate(value))
                        offset = scalars[key]
                        value.ClearField('immediateValue')
                        externalized += 1
                    else:
                        continue
                    value.blobFileValue.fileName = '@model_path/weights/weight.bin'
                    value.blobFileValue.offset = offset
        data = spec.SerializeToString()
        assert b'/Users/' not in data and b'/private/tmp/' not in data
        (OUT / ('body-' + str(bucket) + '.mlmodel')).write_bytes(data)
        print('TEMPLATE', bucket, 'externalized scalars', externalized, flush=True)
    del writer
    with temporary.open('rb') as f:
        headers = [{'offset': 0, 'data': base64.b64encode(f.read(64)).decode()}]
        for s in segments:
            offset = s.pop('metadataOffset')
            f.seek(offset)
            header = f.read(64)
            s['offset'] = struct.unpack_from('<Q', header, 16)[0]
            headers.append({'offset': offset, 'data': base64.b64encode(header).decode()})
    (OUT / 'body-layout.json').write_text(json.dumps({'fileSize': temporary.stat().st_size, 'headers': headers, 'segments': segments}, separators=(',', ':')))
    metadata = {'revision': revision, 'buckets': BUCKETS, 'width': 512, 'dimension': 768, 'imageToken': 258880, 'videoToken': 258884, 'recipes': {p: recipes[p]['model.embed_tokens.weight'] for p in recipes}}
    table = {t.name: t for t in original.graph.initializer}
    for n in original.graph.node:
        if n.name == '/model/multimodal_merge/audio_features/Equal':
            metadata['audioToken'] = int(numpy_helper.to_array(table[n.input[1]], base_dir=str(Path(models_root) / 'fp32/onnx')))
    (OUT / 'metadata.json').write_text(json.dumps(metadata, separators=(',', ':')))
    legacy = Path(vision_assets)
    for name in ['vision.mlmodel', 'vision-layout.json']:
        if (legacy / name).resolve() != (OUT / name).resolve():
            shutil.copy(legacy / name, OUT / name)
    print('COMPLETE', len(segments), temporary.stat().st_size, flush=True)
