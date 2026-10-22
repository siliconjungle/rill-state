#!/usr/bin/env python3
"""Freeze committed comparative inputs and executable identities, without timing builds.

Compilation is required validation work. Retain normal benchmark compiler flags;
dependency source preparation is explicitly outside runtime measurements.
"""
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
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('version')
    parser.add_argument('--features', default='comparisons')
    parser.add_argument('--bench-prefix', default='', help='Dedicated targets, e.g. adaptive_; frozen filenames stay state/shapes')
    parser.add_argument('--layers', default='state,shapes', help='Comma-separated state/shapes targets to freeze')
    args = parser.parse_args()
    assert re.fullmatch(r'v[0-9a-z]+', args.version)
    assert re.fullmatch(r'[a-z0-9_]*', args.bench_prefix)
    layers = args.layers.split(',')
    assert layers and len(layers) == len(set(layers)) and set(layers) <= {'state', 'shapes'}
    root = study.ROOT
    prefix = root / 'evidence' / f'build-{args.version}'
    output = pathlib.Path(str(prefix) + '-receipt.json')
    progress = pathlib.Path(str(prefix) + '-progress.json')
    binary_paths = {name: root / 'build' / f'{name}-{args.version}'
                    for name in layers}
    assert not any(p.exists() for p in [output, progress, *binary_paths.values()])
    commit = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip()
    source_hash = study.source_hash()
    assert source_hash == study.source_hash_at(commit), 'Commit all Cargo/src/bench inputs first'
    environment = dict(os.environ, RUSTUP_TOOLCHAIN='stable', CARGO_BUILD_JOBS='2')
    # Use the local Cargo entry point; direct toolchain Cargo would also need
    # the returned --config arguments, not merely source preparation.
    prepared = json.loads(subprocess.check_output(
        ['ink-cache', 'prepare-sources', str(root), '--offline'],
        cwd=root, env=environment, text=True))
    command = ['cargo', 'bench', '--locked', '--features', args.features]
    for layer in layers:
        command += ['--bench', args.bench_prefix + layer]
    command += ['--no-run', '--message-format=json']
    receipt = dict(source_commit=commit, source_sha256=source_hash,
                   rustc=subprocess.check_output(['rustc', '-Vv'], env=environment, text=True),
                   command=command, environment={k: environment[k] for k in
                       ['RUSTUP_TOOLCHAIN', 'CARGO_BUILD_JOBS']},
                   compiler_environment={k: environment.get(k) for k in
                       ['RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS', 'RUSTC_WRAPPER',
                        'CARGO_TARGET_DIR', 'CARGO_PROFILE_BENCH_OPT_LEVEL',
                        'CARGO_PROFILE_BENCH_LTO', 'CARGO_PROFILE_BENCH_CODEGEN_UNITS']},
                   source_preflight=prepared, timing='Untimed required build; no runtime speed claim',
                   freezer_sha256=hashlib.sha256(pathlib.Path(__file__).read_bytes()).hexdigest(),
                   started_at_unix=time.time())
    progress.write_text(json.dumps(dict(receipt, phase='compiling'), indent=2) + '\n')
    with pathlib.Path(str(prefix) + '.jsonl').open('x') as stdout, \
            pathlib.Path(str(prefix) + '.log').open('x') as stderr:
        completed = subprocess.run(command, cwd=root, env=environment, stdout=stdout, stderr=stderr)
    receipt['build_exit_code'] = completed.returncode
    if completed.returncode:
        progress.write_text(json.dumps(dict(receipt, phase='failed'), indent=2) + '\n')
        raise SystemExit(completed.returncode)
    assert study.source_hash() == source_hash, 'Build inputs changed during compilation'
    artifacts = {}
    for line in pathlib.Path(str(prefix) + '.jsonl').read_text().splitlines():
        event = json.loads(line)
        if event.get('reason') == 'compiler-artifact' and event.get('executable'):
            artifacts[event['target']['name']] = pathlib.Path(event['executable'])
    receipt['binaries'] = {}
    for name, frozen in binary_paths.items():
        with artifacts[args.bench_prefix + name].open('rb') as source, frozen.open('xb') as destination:
            shutil.copyfileobj(source, destination)
        frozen.chmod(0o755)
        receipt['binaries'][str(frozen.relative_to(root))] = hashlib.sha256(frozen.read_bytes()).hexdigest()
    receipt['finished_at_unix'] = time.time()
    with output.open('x') as stream:
        json.dump(receipt, stream, indent=2)
        stream.write('\n')
    progress.write_text(json.dumps(dict(receipt, phase='frozen'), indent=2) + '\n')
    print(json.dumps(dict(receipt=str(output.relative_to(root)), binaries=receipt['binaries'])), flush=True)


if __name__ == '__main__':
    main()
