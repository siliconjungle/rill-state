#!/usr/bin/env python3
"""Freeze committed diagnostic/reclamation examples with independent input receipts."""
import argparse
import hashlib
import json
import os
import pathlib
import re
import shutil
import subprocess
import time

import study


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('version')
    p.add_argument('examples', nargs='+')
    a = p.parse_args()
    assert re.fullmatch(r'v[0-9a-z]+', a.version)
    assert len(set(a.examples)) == len(a.examples)
    assert all(re.fullmatch(r'[a-z_]+', name) for name in a.examples)
    root = study.ROOT
    env = dict(os.environ, RUSTUP_TOOLCHAIN='stable', CARGO_BUILD_JOBS='2')
    commit = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip()
    source_hash = study.source_hash()
    assert source_hash == study.source_hash_at(commit)
    hashes = {}
    destinations = {}
    receipts = {}
    for name in a.examples:
        source = root / 'examples' / f'{name}.rs'
        hashes[name] = hashlib.sha256(source.read_bytes()).hexdigest()
        assert source.read_bytes() == subprocess.check_output(
            ['git', 'show', f'{commit}:examples/{name}.rs'], cwd=root)
        destinations[name] = root / 'build' / f'{name.replace("_", "-")}-{a.version}'
        receipts[name] = root / 'evidence' / f'{name.replace("_", "-")}-{a.version}-build-receipt.json'
    assert not any(p.exists() for p in [*destinations.values(), *receipts.values()])
    prepared = json.loads(subprocess.check_output(
        ['ink-cache', 'prepare-sources', str(root), '--offline'], cwd=root, env=env, text=True))
    command = ['cargo', 'build', '--locked', '--release', '--features', 'comparisons']
    for name in a.examples:
        command += ['--example', name]
    command += ['--message-format=json']
    prefix = root / 'evidence' / f'example-build-{a.version}'
    started = time.time()
    with prefix.with_suffix('.jsonl').open('x') as out, prefix.with_suffix('.log').open('x') as err:
        result = subprocess.run(command, cwd=root, env=env, stdout=out, stderr=err)
    if result.returncode:
        raise SystemExit(result.returncode)
    assert source_hash == study.source_hash()
    events = [json.loads(line) for line in prefix.with_suffix('.jsonl').read_text().splitlines()]
    artifacts = {e['target']['name']: pathlib.Path(e['executable']) for e in events
                 if e.get('reason') == 'compiler-artifact' and e.get('executable')}
    for name, destination in destinations.items():
        assert hashes[name] == hashlib.sha256((root / 'examples' / f'{name}.rs').read_bytes()).hexdigest()
        with artifacts[name].open('rb') as source, destination.open('xb') as target:
            shutil.copyfileobj(source, target)
        destination.chmod(0o755)
        receipt = dict(source_commit=commit, source_sha256=source_hash, example=name,
                       example_sha256=hashes[name], binary=str(destination.relative_to(root)),
                       binary_sha256=hashlib.sha256(destination.read_bytes()).hexdigest(),
                       command=command, source_preflight=prepared,
                       environment={k: env[k] for k in ['RUSTUP_TOOLCHAIN', 'CARGO_BUILD_JOBS']},
                       rustc=subprocess.check_output(['rustc', '-Vv'], env=env, text=True),
                       started_at_unix=started, finished_at_unix=time.time(), build_exit_code=0,
                       timing='Untimed required build; diagnostic runtime and instrumented retention are separate')
        with receipts[name].open('x') as stream:
            json.dump(receipt, stream, indent=2)
            stream.write('\n')
        print(json.dumps(dict(example=name, receipt=str(receipts[name].relative_to(root)))), flush=True)


if __name__ == '__main__':
    main()
