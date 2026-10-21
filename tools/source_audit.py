#!/usr/bin/env python3
"""Record exact published input identity without copying registry trees."""
import hashlib,json,pathlib,tomllib,tarfile
root=pathlib.Path(__file__).resolve().parents[1]
lock=tomllib.loads((root/'Cargo.lock').read_text()); checks={(p['name'],p['version']):p.get('checksum')for p in lock['package']}
rows=[]
for row in json.loads((root/'research/catalog.json').read_text())['crates']:
 if 'version'not in row:continue
 name,version=row['name'],row['version']; folders=list((pathlib.Path.home()/'.cargo/registry/src').glob(f'*/{name}-{version}'))
 if not folders:continue
 folder=folders[0];info=tomllib.loads((folder/'Cargo.toml').read_text());pkg=info['package']; archives=list((pathlib.Path.home()/'.cargo/registry/cache').glob(f'*/{name}-{version}.crate')); assert archives, (name,'published archive missing'); archive=archives[0]; checksum=hashlib.sha256(archive.read_bytes()).hexdigest(); assert checksum==row['checksum'], (name,'archive checksum mismatch'); published=tarfile.open(archive); files={}
 for path in ['Cargo.toml','Cargo.toml.orig','README.md','README','src/lib.rs','src/map.rs','src/art.rs','src/optimistic/write.rs','build.rs']:
  f=folder/path
  if f.is_file():
   actual=hashlib.sha256(f.read_bytes()).hexdigest();expected=hashlib.sha256(published.extractfile(f'{name}-{version}/{path}').read()).hexdigest()
   if expected is not None:assert actual==expected,(name,path,'modified registry source')
   files[path]=actual
 published.close();expected=checks.get((name,version))
 if expected:assert checksum==expected,(name,'lock mismatch')
 rows.append(dict(name=name,version=version,repository=pkg.get('repository'),description=pkg.get('description'),license=pkg.get('license'),source=row['source'],published_checksum=checksum,in_lock=bool(expected),audited_files_sha256=files))
(root/'research/source-inputs.json').write_text(json.dumps(dict(schema=1,note='Published source identity only; semantic capability decisions are separate.',packages=rows),indent=2)+'\n')
print(len(rows),'published packages source-verified')
