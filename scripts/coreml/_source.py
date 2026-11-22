"""Expand the selected source quantization exactly, without re-quantization."""
import numpy as np, onnx
from pathlib import Path
from onnx import helper, numpy_helper

def unpack(x):
    y = np.empty(x.shape[:-1] + (x.shape[-1] * 2,), dtype=np.uint8)
    y[..., 0::2] = x & 15
    y[..., 1::2] = x >> 4
    return y

def load_model(models_root, precision, name='model'):
    suffix = {'q8': '_quantized', 'q4': '_q4', 'fp32': ''}[precision]
    m = onnx.load(str(Path(models_root) / precision / 'onnx' / (name + suffix + '.onnx')))
    initial = {t.name: t for t in m.graph.initializer}
    for node in m.graph.node:
        if node.op_type == 'MatMulNBits':
            a = {v.name: helper.get_attribute_value(v) for v in node.attribute}
            (b, s, z) = [numpy_helper.to_array(initial[x]) for x in node.input[1:]]
            if a['bits'] == 4:
                b = unpack(b)
                z = unpack(z)[..., :s.shape[-1]]
            w = ((b.astype(np.float32) - z[:, :, None]) * s[:, :, None]).reshape(a['N'], -1)[:, :a['K']].T.copy()
            key = node.name + '_dense'
            m.graph.initializer.append(numpy_helper.from_array(w, key))
            x = node.input[0]
            node.op_type = 'MatMul'
            node.domain = ''
            del node.attribute[:]
            del node.input[:]
            node.input.extend([x, key])
        elif node.op_type == 'GatherBlockQuantized':
            a = {v.name: helper.get_attribute_value(v) for v in node.attribute}
            (q, s, z) = [numpy_helper.to_array(initial[x]) for x in [node.input[0], node.input[2], node.input[3]]]
            if a['bits'] == 4:
                q = unpack(q)
                z = unpack(z)[..., :s.shape[-1]]
            w = (q.astype(np.float32) - np.repeat(z, a['block_size'], axis=1)[:, :q.shape[1]]) * np.repeat(s, a['block_size'], axis=1)[:, :q.shape[1]]
            key = node.name + '_dense'
            m.graph.initializer.append(numpy_helper.from_array(w, key))
            idx = node.input[1]
            node.op_type = 'Gather'
            node.domain = ''
            del node.attribute[:]
            node.attribute.append(helper.make_attribute('axis', a['gather_axis']))
            del node.input[:]
            node.input.extend([key, idx])
    used = {v for n in m.graph.node for v in n.input}
    keep = [t for t in m.graph.initializer if t.name in used]
    del m.graph.initializer[:]
    m.graph.initializer.extend(keep)
    return m
