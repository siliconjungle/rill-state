#!/usr/bin/env python3
"""Primary-source crate metadata. No repository or dependency tree copies."""
import concurrent.futures, datetime, json, pathlib, urllib.request, urllib.error, ssl
NAMES = '''im im-rc imbl imbl-rc rpds archery immutable-chunkmap imhm persistent_hashmap persistent-vec rpds-patched pvec immer imstr arc-swap swap-arc aarc triomphe evmap left-right flashmap rcu-clean rcu_cell scc papaya dashmap flurry leapfrog chashmap contrie crossbeam-skiplist skipdb mvcc_cell mvcc-core boxcar elsa redb sled rusqlite heed sanakirja orx-concurrent-vec hashbrown indexmap ordermap btree-slab vart rart crossbeam-epoch seize left-right-copy multiversion'''.split()
DEST = pathlib.Path(__file__).resolve().parents[1] / 'research' / 'catalog.json'
def fetch(name):
    path = ('1/'+name if len(name)==1 else '2/'+name if len(name)==2 else '3/'+name[0]+'/'+name if len(name)==3 else name[:2]+'/'+name[2:4]+'/'+name)
    url = f'https://index.crates.io/{path}'
    req = urllib.request.Request(url, headers={'User-Agent':'rill-state-comparative-research/0.1 (https://github.com/siliconjungle/rill-state)'})
    try:
        with urllib.request.urlopen(req, timeout=30, context=ssl.create_default_context(cafile='/etc/ssl/cert.pem')) as f: data=[json.loads(line) for line in f if line.strip()]
        info=next(v for v in reversed(data) if not v.get('yanked') and '-' not in v['vers']); version=info['vers']
        return dict(name=name, url=url, version=version, checksum=info.get('cksum'), rust_version=info.get('rust_version'), source=f'https://docs.rs/crate/{name}/{version}/source/', dependencies=info.get('deps'), status='discovered')
    except Exception as e: return dict(name=name,url=url,status='metadata_unavailable',error=str(e))
if __name__=='__main__':
    with concurrent.futures.ThreadPoolExecutor(max_workers=6) as pool: rows=list(pool.map(fetch,NAMES))
    DEST.write_text(json.dumps(dict(fetched_at=datetime.datetime.now(datetime.timezone.utc).isoformat(), crates=rows),indent=2)+'\n')
    for row in rows: print(row['name'],row.get('version',row['status']))
