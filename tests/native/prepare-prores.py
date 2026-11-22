"""Generate six deterministic ProRes profiles; optionally fetch public FFmpeg FATE samples."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import urllib.request

resources = Path(__file__).resolve().parents[2] / '.local/test-media'
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--ffmpeg', default=os.environ.get('CLIPSTUDIO_FFMPEG'), required=not os.environ.get('CLIPSTUDIO_FFMPEG'))
parser.add_argument('--output', type=Path, default=resources / 'prores')
parser.add_argument('--download', action='store_true', help='Download ~1 MB of public FATE samples')
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=True)
for profile, name in enumerate(['proxy', 'lt', 'standard', 'hq', '4444', '4444xq']):
    target = args.output / (name + '.mov')
    if target.exists():
        continue
    video = 'testsrc2=size=640x360:rate=25'
    if profile >= 4:
        video += ",format=yuva444p10le,geq=lum='lum(X,Y)':cb='cb(X,Y)':cr='cr(X,Y)':a='1023*X/W'"
    temporary = target.with_suffix('.tmp.mov')
    try:
        subprocess.run([args.ffmpeg, '-v', 'error', '-y', '-f', 'lavfi', '-i', video,
                        '-f', 'lavfi', '-i', 'sine=frequency=440:sample_rate=48000', '-t', '8',
                        '-c:v', 'prores_ks', '-profile:v', str(profile), '-threads', '2',
                        '-pix_fmt', 'yuv422p10le' if profile < 4 else 'yuva444p10le',
                        '-c:a', 'pcm_s16le', '-timecode', '10:00:00:00', str(temporary)], check=True)
        temporary.rename(target)
    finally:
        temporary.unlink(missing_ok=True)
    print('Generated', target)
if args.download:
    public = args.output / 'public'
    public.mkdir(exist_ok=True)
    records = []
    names = ['Sequence_1-Apple_ProRes_422.mov', 'Sequence_1-Apple_ProRes_422_HQ.mov',
             'Sequence_1-Apple_ProRes_422_LT.mov', 'Sequence_1-Apple_ProRes_422_Proxy.mov',
             'Sequence_1-Apple_ProRes_with_Alpha.mov', 'prores4444_with_transparency.mov']
    for name in names:
        url = 'https://fate-suite.ffmpeg.org/prores/' + name
        target = public / name
        if not target.exists():
            with urllib.request.urlopen(url, timeout=30) as response:
                data = response.read(4 * 1024 * 1024)
                if len(data) >= 4 * 1024 * 1024:
                    raise ValueError('Unexpectedly large sample')
                target.write_bytes(data)
        records.append({'file': name, 'url': url, 'bytes': target.stat().st_size,
                        'sha256': hashlib.sha256(target.read_bytes()).hexdigest()})
    (public / 'sources.json').write_text(json.dumps(records, indent=2) + '\n')
