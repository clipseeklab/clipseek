"""Map verified source graph initializers to byte spans without copying weights."""
import onnx
from onnx import helper
from pathlib import Path
def fields(data, start=0, end=None):
    end = len(data) if end is None else end

    def var(pos):
        v = 0
        s = 0
        while True:
            b = data[pos]
            pos += 1
            v |= (b & 127) << s
            s += 7
            if b < 128:
                return (v, pos)
    pos = start
    while pos < end:
        (key, pos) = var(pos)
        wire = key & 7
        tag = key >> 3
        if wire == 2:
            (size, pos) = var(pos)
            a = pos
            pos += size
        elif wire == 0:
            a = pos
            (_, pos) = var(pos)
        else:
            a = pos
            pos += 8 if wire == 1 else 4
        yield (tag, a, pos)

def source_model(models_root, precision, stem):
    suffix = {'q8': '_quantized', 'q4': '_q4', 'fp32': ''}[precision]
    name = stem + suffix + '.onnx'
    path = str(Path(models_root) / precision / 'onnx' / name)
    data = open(path, 'rb').read()
    model = onnx.load(path, load_external_data=False)
    raw = {}
    for (tag, a, b) in fields(data):
        if tag != 7:
            continue
        for (tag, c, d) in fields(data, a, b):
            if tag != 5:
                continue
            parts = list(fields(data, c, d))
            key = next((data[x:y].decode() for (tag, x, y) in parts if tag == 8))
            for (tag, x, y) in parts:
                if tag == 9:
                    raw[key] = {'file': name, 'offset': x, 'bytes': y - x}
    tensors = {t.name: t for t in model.graph.initializer}

    def ref(key):
        t = tensors[key]
        external = {e.key: e.value for e in t.external_data}
        r = {'file': external['location'], 'offset': int(external.get('offset', 0)), 'bytes': int(external['length'])} if external else raw.get(key)
        if r is None:
            raise ValueError('Unsupported source ' + key)
        return dict(r, dtype={1: 'float32', 7: 'int64', 2: 'uint8', 6: 'int32'}[t.data_type], shape=list(t.dims))
    return (model, ref)

def recipe_maps(models_root, stem):
    (base, base_ref) = source_model(models_root, 'fp32', stem)
    base_nodes = {n.name: n for n in base.graph.node}
    maps = {}
    for precision in ['q8', 'q4', 'fp32']:
        (m, ref) = source_model(models_root, precision, stem)
        rs = {t.name: dict(kind='plain', data=ref(t.name)) for t in m.graph.initializer if t.name in {t.name for t in base.graph.initializer}}
        for n in m.graph.node:
            if n.op_type not in ['MatMulNBits', 'GatherBlockQuantized']:
                continue
            name = n.name.removesuffix('_Q4').removesuffix('_quant').removesuffix('_Quant')
            if name not in base_nodes:
                raise ValueError((precision, n.name))
            original = base_nodes[name]
            attrs = {a.name: helper.get_attribute_value(a) for a in n.attribute}
            if n.op_type == 'MatMulNBits':
                key = original.input[1]
                rs[key] = {'kind': 'matmul', 'bits': attrs['bits'], 'k': attrs['K'], 'n': attrs['N'], 'block': attrs['block_size'], 'data': ref(n.input[1]), 'scales': ref(n.input[2]), 'zeros': ref(n.input[3])}
            else:
                key = original.input[0]
                rs[key] = {'kind': 'gather', 'bits': attrs['bits'], 'block': attrs['block_size'], 'data': ref(n.input[0]), 'scales': ref(n.input[2]), 'zeros': ref(n.input[3])}
        maps[precision] = rs
    return maps
