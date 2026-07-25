#!/usr/bin/env python3
"""ChituBox .chitubox differential recon (analysis-only, NOT product code).

Provenance for docs/specs/CHITUBOX-FORMAT.md. Run against the corpus in
"/Users/craig/Documents/Claude/Projects/D&D Campaigns/". Reference tooling
only -- the product is pure Rust (rust/chitubox_io).
"""
import struct, sys
import numpy as np

CORPUS = "/Users/craig/Documents/Claude/Projects/D&D Campaigns"
MAGIC_FILE = 0xAB231243          # header magic, off 0
MAGIC_REC  = 3929285513          # 0xEA342389, support segment record magic

def u32(b, o): return struct.unpack_from('<I', b, o)[0]
def f32(b, o): return struct.unpack_from('<f', b, o)[0]

def load(name): return open(f"{CORPUS}/{name}", 'rb').read()

def header(d):
    return dict(magic=u32(d,0), model_count=u32(d,4), model_table_off=u32(d,8),
                settings_size=u32(d,12), const16=u32(d,16), support_sec_off=u32(d,20))

def models(d):
    """Yield (index, name, record_bytes, mesh_off, mesh_size, tx,ty,tz)."""
    h = header(d); base = h['model_table_off']
    for i in range(h['model_count']):
        st = base + i*680
        r = d[st:st+680]
        name = r[:32].split(b'\x00')[0].decode('latin1')
        yield dict(i=i, off=st, name=name,
                   tx=f32(r,256), ty=f32(r,260), tz=f32(r,264),
                   mesh_off=u32(r,272), mesh_size=u32(r,276))

def mesh_tris(d, mo, ms):
    """Embedded mesh = 36 B/tri = 9 float32 (3 verts, NO normal)."""
    return np.frombuffer(d[mo:mo+ms], dtype='<f4').reshape(-1, 9)

def stl_tris(name):
    """Binary STL: 80 hdr + u32 count + count*(50 = 3f normal + 9f verts + u16)."""
    d = load(name); n = u32(d, 80)
    return np.array([struct.unpack_from('<9f', d, 84+i*50+12) for i in range(n)]).reshape(-1, 9)

def support_records(d):
    pat = struct.pack('<I', MAGIC_REC); hits = []; o = 0
    while True:
        i = d.find(pat, o)
        if i < 0: break
        hits.append(i); o = i+1
    return hits

def decode_record(d, b):
    return dict(off=b, magic=u32(d,b), type=u32(d,b+4),
                start=(f32(d,b+8), f32(d,b+12), f32(d,b+16)),
                end=(f32(d,b+20), f32(d,b+24), f32(d,b+28)),
                r1=f32(d,b+32), r2=f32(d,b+36),
                mesh_off=u32(d,b+40), mesh_len=u32(d,b+44))

def find_index(d, rec_off, n):
    """Per-model 16-byte index: (n_records, records_off, mesh_off, mesh_size)."""
    i = d.find(struct.pack('<II', n, rec_off))
    if i < 0: return None
    return dict(off=i, n=u32(d,i), rec_off=u32(d,i+4),
                mesh_off=u32(d,i+8), mesh_size=u32(d,i+12))

if __name__ == "__main__":
    for name in ["empty","sphere-cylinder","sphere-cylinder-supported",
                 "sphere-moved","sphere-scaled","cylinder-rotated"]:
        d = load(name+".chitubox"); h = header(d)
        print(f"\n== {name} ({len(d)} B) magic={h['magic']:#x} mc={h['model_count']} "
              f"table@{h['model_table_off']} supsec@{h['support_sec_off']}")
        for m in models(d):
            print(f"   [{m['i']}] {m['name']:16s} t=({m['tx']:.3f},{m['ty']:.3f},{m['tz']:.3f}) "
                  f"mesh@{m['mesh_off']} +{m['mesh_size']} ({m['mesh_size']//36} tris)")
