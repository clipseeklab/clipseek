"""Developer-only MIL adapter for the pinned ONNX graphs, not a general converter."""
import os, sys, json, math, numpy as np, onnx, coremltools as ct
from onnx import numpy_helper, helper
from coremltools.converters.mil import Builder as mb
from coremltools.converters.mil.mil import types, curr_block

def export_model(m, root, kind='text', bucket=512):
    parents = {out: i for (i, n) in enumerate(m.graph.node) for out in n.output}
    keep_nodes = set()

    def mark(name):
        if name not in parents:
            return
        i = parents[name]
        if i in keep_nodes:
            return
        keep_nodes.add(i)
        for x in m.graph.node[i].input:
            mark(x)
    for (i, n) in enumerate(m.graph.node):
        if n.op_type in ['Cos', 'Sin']:
            keep_nodes.add(i)
            for x in n.input:
                mark(x)
        if kind == 'text' and ('/model/pooling/' in n.name or '/model/attention_bias/' in n.name or n.op_type in ['SimplifiedLayerNormalization', 'SkipSimplifiedLayerNormalization', 'Softmax']):
            keep_nodes.add(i)
    arrays = {t.name: numpy_helper.to_array(t) for t in m.graph.initializer}
    source_names = list(arrays)
    constant_meta = {}
    env = {}

    def clean(x):
        x = np.asarray(x)
        if x.dtype == np.int64:
            return np.clip(x, -2147483648, 2147483647).astype(np.int32)
        if x.dtype in [np.float32, np.float64]:
            return np.clip(x, -65504, 65504).astype(np.float32)
        return x
    for (key, value) in list(arrays.items()):
        arrays[key] = clean(value)

    def value(x):
        if hasattr(x, 'val'):
            if x.val is None:
                raise ValueError('Expected constant ' + x.name)
            x = x.val
        return np.asarray(x)

    def scalar(x):
        return value(x).item()

    def const(x, name=None):
        return mb.const(val=clean(x), **{'name': name} if name else {})

    def shape(x):
        return mb.shape(x=x)

    def take(x, indices):
        return mb.gather(x=x, indices=np.asarray(indices, np.int32), axis=0)

    def reshape(x, s):
        return mb.reshape(x=x, shape=np.asarray(s, np.int32) if isinstance(s, (list, tuple)) else s)

    def rms(x, w, e):
        mean = mb.reduce_mean(x=mb.square(x=x), axes=[-1], keep_dims=True)
        return mb.mul(x=mb.mul(x=x, y=mb.rsqrt(x=mb.add(x=mean, y=np.float32(e)))), y=w)

    def sliced(x, starts, ends, axes, steps):
        axes = value(axes).reshape(-1).tolist()
        strides = value(steps).reshape(-1).tolist()
        rank = x.rank
        beg = []
        end = []
        step = [1] * rank
        for axis in range(rank):
            matches = [j for (j, a) in enumerate(axes) if a % rank == axis]
            if matches:
                j = matches[0]
                size = take(shape(x), [axis])
                b = take(starts, [j])
                e = take(ends, [j])
                step[axis] = strides[j]
                if step[axis] > 0:
                    b = mb.select(cond=mb.less(x=b, y=0), a=mb.add(x=b, y=size), b=b)
                    e = mb.select(cond=mb.less(x=e, y=0), a=mb.add(x=e, y=size), b=e)
                    b = mb.minimum(x=mb.maximum(x=b, y=0), y=size)
                    e = mb.minimum(x=mb.maximum(x=e, y=0), y=size)
                beg.append(b)
                end.append(e)
            else:
                beg.append(const([0]))
                end.append(take(shape(x), [axis]))
        return mb.slice_by_index(x=x, begin=mb.concat(values=beg, axis=0), end=mb.concat(values=end, axis=0), stride=np.asarray(step, np.int32))

    def select(cond, a, b):
        from coremltools.converters.mil.mil.ops.defs._utils import broadcast_shapes
        branch = broadcast_shapes(a.shape, b.shape)
        if tuple(broadcast_shapes(branch, cond.shape)) == tuple(branch):
            return mb.select(cond=cond, a=a, b=b)
        if a.val is not None and b.val is not None and (a.rank == 0) and (b.rank == 0) and (float(a.val) == 0) and (float(b.val) == -65504):
            return mb.mul(x=mb.cast(x=mb.logical_not(x=cond), dtype='fp32'), y=np.float32(-65504))
        if cond.rank:
            zero = mb.fill(shape=shape(cond), value=np.int32(0) if a.dtype in [types.int32, types.bool] else np.float32(0))
            if a.dtype == types.bool:
                zero = mb.cast(x=zero, dtype='bool')
                a = mb.logical_or(x=a, y=zero)
                b = mb.logical_or(x=b, y=zero)
            else:
                a = mb.add(x=a, y=zero)
                b = mb.add(x=b, y=zero)
        return mb.select(cond=cond, a=a, b=b)

    def convert(inputs):
        if kind == 'text':
            embedded = inputs[0]
            env['input_ids'] = inputs[1]
            env['attention_mask'] = inputs[2]
        else:
            env.update(dict(zip([i.name for i in m.graph.input], inputs)))
        for (i, key) in enumerate(source_names):
            env[key] = const(arrays[key], name='w' + str(i))
            constant_meta['w' + str(i)] = key
        for (idx, node) in enumerate(m.graph.node):
            if kind == 'text' and any((node.name.endswith('/attn/' + suffix) for suffix in ['MatMul_qk', 'scores/Reshape', 'Add_bias', 'Softmax', 'probs/Reshape'])):
                continue
            if kind == 'text' and (node.name.startswith('/model/embed_tokens/Gather') or node.name.startswith('/model/multimodal_merge/')):
                env['/model/multimodal_merge/Reshape_embeds/output_0'] = embedded
                continue
            before = len(curr_block().operations)
            op = node.op_type
            a = {at.name: helper.get_attribute_value(at) for at in node.attribute}
            v = [env.get(n) if n else None for n in node.input]
            r = None
            try:
                if op == 'Shape':
                    r = shape(v[0])
                    start = a.get('start', 0)
                    end = a.get('end', v[0].rank)
                    if start != 0 or end != v[0].rank:
                        r = take(r, list(range(v[0].rank))[start:end])
                elif op == 'Size':
                    r = mb.reduce_prod(x=shape(v[0]))
                elif op in ['Add', 'Sub', 'Mul', 'Div', 'Equal', 'Greater', 'LessOrEqual', 'And', 'Max']:
                    f = {'Add': mb.add, 'Sub': mb.sub, 'Mul': mb.mul, 'Div': mb.real_div if types.is_float(v[0].dtype) else mb.floor_div, 'Equal': mb.equal, 'Greater': mb.greater, 'LessOrEqual': mb.less_equal, 'And': mb.logical_and, 'Max': mb.maximum}[op]
                    r = f(x=v[0], y=v[1])
                elif op == 'Where':
                    r = select(v[0], v[1], v[2])
                elif op in ['Abs', 'Neg', 'Not', 'Cos', 'Sin']:
                    r = {'Abs': mb.abs, 'Neg': lambda x: mb.mul(x=x, y=np.float32(-1) if types.is_float(x.dtype) else np.int32(-1)), 'Not': mb.logical_not, 'Cos': mb.cos, 'Sin': mb.sin}[op](x=v[0])
                elif op == 'Cast':
                    r = mb.cast(x=v[0], dtype={1: 'fp32', 6: 'int32', 7: 'int32', 9: 'bool'}[a['to']])
                elif op == 'Clip':
                    r = v[0]
                    if len(v) > 1 and v[1] is not None:
                        r = mb.maximum(x=r, y=v[1])
                    if len(v) > 2 and v[2] is not None:
                        r = mb.minimum(x=r, y=v[2])
                elif op == 'Gather':
                    r = mb.gather(x=v[0], indices=v[1], axis=a.get('axis', 0))
                elif op == 'GatherND':
                    r = reshape(v[0], [-1, int(v[0].shape[-1])]) if kind == 'vision' else mb.gather_nd(x=v[0], indices=v[1], batch_dims=a.get('batch_dims', 0))
                elif op == 'Concat':
                    r = mb.concat(values=v, axis=a['axis'])
                elif op == 'Unsqueeze':
                    r = mb.expand_dims(x=v[0], axes=value(v[1]).astype(np.int32) if len(v) > 1 else a['axes'])
                elif op == 'Squeeze':
                    r = mb.squeeze(x=v[0], axes=value(v[1]).astype(np.int32) if len(v) > 1 else a.get('axes'))
                elif op == 'Reshape':
                    s = v[1]
                    if not a.get('allowzero', 0) and s.shape[0] != 0:
                        n = s.shape[0]
                        original = shape(v[0])
                        extras = max(0, int(n) - v[0].rank)
                        if extras:
                            original = mb.concat(values=[original, np.ones(extras, np.int32)], axis=0)
                        original = take(original, list(range(int(n))))
                        s = mb.select(cond=mb.equal(x=s, y=0), a=original, b=s)
                    r = reshape(v[0], s)
                elif op == 'Transpose':
                    r = mb.transpose(x=v[0], perm=a.get('perm', list(range(v[0].rank))[::-1]))
                elif op == 'Expand':
                    fill = mb.fill(shape=v[1], value=False if v[0].dtype == types.bool else 0 if v[0].dtype == types.int32 else np.float32(0))
                    r = mb.logical_or(x=v[0], y=fill) if v[0].dtype == types.bool else mb.add(x=v[0], y=fill)
                elif op == 'Range':
                    r = inputs[3] if kind == 'text' and node.name == '/model/positions/Range' else mb.range_1d(start=reshape(v[0], []), end=reshape(v[1], []), step=reshape(v[2], []))
                elif op == 'Slice':
                    r = sliced(v[0], v[1], v[2], v[3] if len(v) > 3 else const(list(range(v[1].shape[0]))), v[4] if len(v) > 4 else const([1] * v[1].shape[0]))
                elif op == 'Pad':
                    pads = v[1]
                    n = v[0].rank
                    order = [j for i in range(n) for j in [i, i + n]]
                    pads = take(pads, order)
                    r = mb.pad(x=v[0], pad=pads, mode=a.get('mode', b'constant').decode(), constant_val=scalar(v[2]) if len(v) > 2 and v[2] is not None else 0.0)
                elif op in ['ReduceSum', 'ReduceMax', 'ReduceL2']:
                    axes = value(v[1]).astype(np.int32) if len(v) > 1 else a.get('axes')
                    f = {'ReduceSum': mb.reduce_sum, 'ReduceMax': mb.reduce_max, 'ReduceL2': mb.reduce_l2_norm}[op]
                    r = f(x=v[0], axes=axes, keep_dims=bool(a.get('keepdims', 1)))
                elif op == 'MatMul':
                    if node.name.endswith('/attn/MatMul_v'):
                        prefix = node.name.removesuffix('MatMul_v')
                        q = env[prefix + 'q/Transpose/output_0']
                        k = mb.expand_dims(x=mb.transpose(x=env[prefix + 'k/Transpose/output_0'], perm=[0, 1, 3, 2]), axes=[2])
                        z = mb.expand_dims(x=env[prefix + 'v/Transpose/output_0'], axes=[2])
                        bias_name = next((n.input[1] for n in m.graph.node if n.name == prefix + 'Add_bias'))
                        shape_name = next((n.input[1] for n in m.graph.node if n.name == prefix + 'q/Reshape'))
                        (kv, groups, d) = map(int, arrays[shape_name][2:])
                        heads = kv * groups
                        r = reshape(mb.scaled_dot_product_attention(query=mb.mul(x=reshape(q, [1, heads, -1, d]), y=np.float32(math.sqrt(d))), key=reshape(mb.tile(x=k, reps=[1, 1, groups, 1, 1]), [1, heads, -1, d]), value=reshape(mb.tile(x=z, reps=[1, 1, groups, 1, 1]), [1, heads, -1, d]), attn_mask=mb.squeeze(x=env[bias_name], axes=[1])), [1, kv, -1, d])
                    else:
                        r = mb.matmul(x=v[0], y=v[1])
                elif op == 'Gemm':
                    r = mb.matmul(x=v[0], y=v[1], transpose_x=bool(a.get('transA', 0)), transpose_y=bool(a.get('transB', 0)))
                    if a.get('alpha', 1) != 1:
                        r = mb.mul(x=r, y=np.float32(a['alpha']))
                    if len(v) > 2:
                        r = mb.add(x=r, y=mb.mul(x=v[2], y=np.float32(a.get('beta', 1))))
                elif op == 'SimplifiedLayerNormalization':
                    r = rms(v[0], v[1], a.get('epsilon', 1e-05))
                elif op == 'SkipSimplifiedLayerNormalization':
                    total = mb.add(x=v[0], y=v[1])
                    y = rms(total, v[2], a.get('epsilon', 1e-05))
                    r = (y, y, y, total) if len(node.output) > 1 else y
                elif op == 'Gelu':
                    r = mb.gelu(x=v[0], mode='TANH_APPROXIMATION' if a.get('approximate', b'none') == b'tanh' else 'EXACT')
                elif op == 'ScatterElements':
                    r = mb.scatter_along_axis(data=v[0], indices=v[1], updates=v[2], axis=a.get('axis', 0), mode='add' if a.get('reduction', b'none') == b'add' else 'update')
                elif op == 'NonZero':
                    r = mb.transpose(x=mb.non_zero(x=v[0]), perm=[1, 0])
                elif op == 'Split':
                    r = mb.split(x=v[0], split_sizes=value(v[1]).astype(np.int32) if len(v) > 1 else None, num_splits=len(node.output) if len(v) == 1 else None, axis=a.get('axis', 0))
                elif op == 'Softmax':
                    r = mb.softmax(x=v[0], axis=a.get('axis', -1))
                elif op == 'CumSum':
                    r = mb.cumsum(x=v[0], axis=int(scalar(v[1])), exclusive=bool(a.get('exclusive', 0)), reverse=bool(a.get('reverse', 0)))
                elif op == 'MultiHeadAttention':
                    heads = a['num_heads']
                    d = v[0].shape[-1] // heads
                    (q, k, z) = [mb.transpose(x=reshape(t, [1, -1, heads, d]), perm=[0, 2, 1, 3]) for t in v[:3]]
                    score = mb.mul(x=mb.matmul(x=q, y=k, transpose_y=True), y=np.float32(a.get('scale', 1 / math.sqrt(d))))
                    r = reshape(mb.transpose(x=mb.matmul(x=mb.softmax(x=score, axis=-1), y=z), perm=[0, 2, 1, 3]), [1, -1, heads * d])
                elif op == 'RotaryEmbedding':
                    assert a.get('interleaved', 0) == 0 and np.all(value(v[1]) == 0)
                    h = int(v[2].shape[-1])
                    hidden = int(v[0].shape[-1])
                    t = reshape(v[0], [1, -1, hidden // (2 * h), 2 * h])
                    (left, right) = mb.split(x=t, split_sizes=[h, h], axis=-1)
                    c = reshape(v[2], [1, -1, 1, h])
                    s = reshape(v[3], [1, -1, 1, h])
                    r = reshape(mb.concat(values=[mb.sub(x=mb.mul(x=left, y=c), y=mb.mul(x=right, y=s)), mb.add(x=mb.mul(x=right, y=c), y=mb.mul(x=left, y=s))], axis=-1), [1, -1, hidden])
                else:
                    raise NotImplementedError(op)
            except Exception:
                print('FAILED NODE', idx, node.name, op, [getattr(x, 'shape', None) for x in v], flush=True)
                raise
            if idx in keep_nodes:
                for created in list(curr_block().operations)[before:]:
                    created.name = 'keepfp32_' + created.name
            if len(node.output) == 1:
                env[node.output[0]] = r
            else:
                for (key, out) in zip(node.output, r):
                    if key:
                        env[key] = out
            target = m.graph.output[0].name if kind == 'vision' else '/model/pooling/Div_mean/output_0'
            if target in env:
                return mb.identity(x=env[target], name='features' if kind == 'vision' else 'pooled_embedding')
        raise ValueError('Missing output')
    if kind == 'vision':
        p = 2520

        @mb.program(input_specs=[mb.TensorSpec(shape=(1, p, 768)), mb.TensorSpec(shape=(1, p, 2), dtype=types.int32)], opset_version=ct.target.iOS18)
        def prog(pixel_values, pixel_position_ids):
            return convert([pixel_values, pixel_position_ids])
        inputs = [ct.TensorType(name='pixel_values', shape=(1, p, 768), dtype=np.float32), ct.TensorType(name='pixel_position_ids', shape=(1, p, 2), dtype=np.int32)]
    else:
        l = bucket

        @mb.program(input_specs=[mb.TensorSpec(shape=(1, l, 512)), mb.TensorSpec(shape=(1, l), dtype=types.int32), mb.TensorSpec(shape=(1, l), dtype=types.int32), mb.TensorSpec(shape=(l,))], opset_version=ct.target.iOS18)
        def prog(embedded_tokens, input_ids, attention_mask, positions):
            return convert([embedded_tokens, input_ids, attention_mask, positions])
        inputs = [ct.TensorType(name='embedded_tokens', shape=(1, l, 512), dtype=np.float32), ct.TensorType(name='input_ids', shape=(1, l), dtype=np.int32), ct.TensorType(name='attention_mask', shape=(1, l), dtype=np.int32), ct.TensorType(name='positions', shape=(l,), dtype=np.float32)]
    print('BUILT', kind, flush=True)
    pipeline = ct.PassPipeline.DEFAULT
    pipeline.remove_passes(['common::fuse_matmul_weight_bias', 'common::fuse_transpose_matmul', 'common::fuse_linear_bias'])
    model = ct.convert(prog, pass_pipeline=pipeline, inputs=inputs, minimum_deployment_target=ct.target.macOS15, compute_precision=ct.transform.FP16ComputePrecision(op_selector=lambda op: not op.name.startswith('keepfp32_')), compute_units=ct.ComputeUnit.CPU_ONLY, skip_model_load=True, outputs=[ct.TensorType(dtype=np.float32)])
    model.save(root + '/' + kind + '.mlpackage')
    json.dump(constant_meta, open(root + '/' + kind + '-sources.json', 'w'))
    print('SAVED', kind, flush=True)
