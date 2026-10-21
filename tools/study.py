#!/usr/bin/env python3
"""Parallel full-contract proofs and broad development screening.

Parallel timings are selection evidence only. Final superiority claims require
fresh serial processes, holdout seeds and the separate strict acceptance gate.
"""
import argparse, concurrent.futures, hashlib, itertools, json, os, pathlib, platform, random, re, subprocess, time
ROOT=pathlib.Path(__file__).resolve().parents[1]
EXTERNAL=['std-hash','std-btree','hashbrown','hashbrown-sip','indexmap','btree-slab','im-hash','im-ord','imbl-hash','imbl-ord','imhm','imhm-sip','rpds-hash','rpds-redblack','vart','papaya-copy','dashmap-copy','flurry-copy','skipdb']+[f'chunk-{n}' for n in [8,16,32,64,128]]+[f'chunkbatch-{n}' for n in [8,16,32,64,128]]
INDEPENDENT=[f'{family}-{n}' for family in ['hashpages','sortedpages'] for n in [16,64,256,1024,4096,16384]]+[f'overlay-{n}' for n in [1,2,4,8,16,32]]+[f'mvcc-{n}' for n in [1,16,64,256]]+[f'slots-{n}'for n in [8,16,32,64,128,256,512,1024]]
RILL=[f'rill-{1<<bits}-{1<<branch}' for bits in range(6,17,2) for branch in range(0,min(bits,10)+1,2)]
RILLFAST=[e.replace('rill-','rillfast-')for e in RILL]
ENGINES=EXTERNAL+INDEPENDENT+RILL+RILLFAST
CASES=['idle','single','sparse','hot','churn','burst','replace','slow','read','scan','acquire','feed']
def source_hash():
    h=hashlib.sha256()
    for p in sorted([ROOT/'Cargo.toml',ROOT/'Cargo.lock',*ROOT.glob('src/**/*.rs'),*ROOT.glob('benches/**/*.rs')]): h.update(p.relative_to(ROOT).as_posix().encode()+b'\0'+p.read_bytes())
    return h.hexdigest()
def execute(task):
    binary,engine,case,seed,records,readers,rounds,proof=task
    cmd=[str(binary),'--engine',engine,'--case',case,'--seed',str(seed),'--records',str(records),'--readers',str(readers),'--rounds',str(rounds)]
    if proof: cmd+=['--verify']
    wrapper=['/usr/bin/time','-l','-p'] if platform.system()=='Darwin' else ['/usr/bin/time','-v']
    start=time.time(); result=subprocess.run(wrapper+cmd,capture_output=True,text=True,timeout=300)
    if result.returncode: raise RuntimeError(json.dumps(dict(command=cmd,code=result.returncode,stderr=result.stderr[-3000:],stdout=result.stdout[-3000:])))
    row=json.loads(result.stdout)
    if platform.system()=='Darwin':
        m=re.search(r'(\d+)\s+maximum resident set size',result.stderr)
        if m: row['max_rss_bytes']=int(m[1])
        for field in ['user','sys']:
            m=re.search(r'^'+field+r'\s+([0-9.]+)',result.stderr,re.M)
            if m:row[field+'_seconds']=float(m[1])
    else:
        m=re.search(r'Maximum resident set size \(kbytes\): (\d+)',result.stderr)
        if m:row['max_rss_bytes']=int(m[1])*1024
    row['started_at_unix']=start
    return row
def main():
    p=argparse.ArgumentParser();p.add_argument('mode',choices=['prove','screen','serial']);p.add_argument('--binary',type=pathlib.Path,required=True);p.add_argument('--label',required=True);p.add_argument('--workers',type=int,default=3);p.add_argument('--engines');p.add_argument('--cases');p.add_argument('--records',type=int,default=20000);p.add_argument('--readers',type=int,default=20000);p.add_argument('--rounds',type=int,default=12);p.add_argument('--repetitions',type=int,default=1);p.add_argument('--seeds',default='4815162342,2937471921');a=p.parse_args()
    assert re.fullmatch(r'[A-Za-z0-9_.-]+',a.label)
    engines=a.engines.split(',')if a.engines else ENGINES
    cases=a.cases.split(',')if a.cases else CASES
    seeds=[int(s)for s in a.seeds.split(',')]
    binary=a.binary.resolve(); binary_hash=hashlib.sha256(binary.read_bytes()).hexdigest()
    folder=ROOT/'evidence'/a.label;folder.mkdir(parents=True,exist_ok=False)
    tasks=[(binary,e,c,s,a.records,a.readers,a.rounds,a.mode=='prove')for _ in range(a.repetitions)for e,c,s in itertools.product(engines,cases,seeds)]
    random.Random(0x51A7E).shuffle(tasks)
    workers=1 if a.mode=='serial'else a.workers
    receipt=dict(schema=1,mode=a.mode,claim='correctness'if a.mode=='prove'else'selection only'if a.mode=='screen'else'isolated timing; acceptance not implied',workers=workers,source_sha256=source_hash(),orchestrator_sha256=hashlib.sha256(pathlib.Path(__file__).read_bytes()).hexdigest(),binary_sha256=binary_hash,platform=platform.platform(),machine=platform.machine(),rustc=subprocess.check_output(['rustc','-Vv'],text=True),engines=engines,cases=cases,seeds=seeds,records=a.records,readers=a.readers,rounds=a.rounds,repetitions=a.repetitions,tasks=len(tasks))
    (folder/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n')
    passed=0;errors=[]
    with (folder/'runs.jsonl').open('w')as out,concurrent.futures.ThreadPoolExecutor(max_workers=workers)as pool:
        futures={pool.submit(execute,t):t for t in tasks}
        for future in concurrent.futures.as_completed(futures):
            try: row=future.result();out.write(json.dumps(row)+'\n');out.flush();passed+=1
            except Exception as e:errors.append(dict(task=list(map(str,futures[future])),error=str(e)));(folder/'errors.json').write_text(json.dumps(errors,indent=2)+'\n')
            if (passed+len(errors))%50==0:print(f'{passed} completed, {len(errors)} failures / {len(tasks)}',flush=True)
    receipt.update(completed=passed,failures=len(errors),finished_at_unix=time.time());(folder/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n')
    # Equal logical outputs across engines, independently of any ranking. Ordered
    # mutation vectors and complete snapshots are checked by the proof mode itself.
    outputs={}
    if a.mode!='prove':
        for line in (folder/'runs.jsonl').read_text().splitlines():
            r=json.loads(line);k=(r['case'],r['seed']);signature=(r['checksum'],r['read_checksum'],r['final_len'])
            if k in outputs and outputs[k]!=signature:errors.append(dict(output_mismatch=k,expected=outputs[k],actual=signature,engine=r['engine']))
            outputs[k]=signature
    receipt['output_agreement']=not errors;(folder/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n')
    if errors:(folder/'errors.json').write_text(json.dumps(errors,indent=2)+'\n')
    print(json.dumps(dict(label=a.label,engines=len(engines),completed=passed,errors=len(errors))),flush=True)
    return int(bool(errors))
if __name__=='__main__':raise SystemExit(main())
