#!/usr/bin/env python3
"""Instrumented payload retention/reclamation proof; never a speed ranking."""
import collections
import argparse,concurrent.futures,hashlib,itertools,json,pathlib,platform,random,re,subprocess
import study
ROOT=study.ROOT
ENGINES=['hashbrown','imbl-fast','probe-paged16','shared-paged16','record-paged16','record-flat','rillfast']

def validate_row(row):
    assert row['verified'] and row['instrumented'] and row['after_all_drop']==0
    # Independently derive the exact journal holdings from submitted batch lengths.
    lengths=collections.deque();operations=0
    for size in [row['records']]+[row['changed']]*row['rounds']:
        lengths.append(size);operations+=size
        while len(lengths)>64 or operations>1_000_000:operations-=lengths.popleft()
    assert (row['history_batches'],row['history_operations'])==(len(lengths),operations)
    assert row['peak_owned_payload_bytes']>=row['owned_payload_bytes_end']>=row['owned_payload_bytes_after_unpin']
    assert row['live_values_after_window_drop']<=row['live_values_after_unpin']<=row['live_values_end']
    if 'scans_per_revision' in row:
        assert row['scan_value_clones']==0
        assert row['cached_records_current'] in [0,row['records']]
        if row['scans_per_revision']==0:
            assert row['cached_records_distinct_revisions']==0
        if row['engine'].startswith(('adaptive-cache-', 'adaptive-reusecache-', 'adaptive-copyaddr-', 'adaptive-reuseaddr-', 'adaptive-copybucket-', 'adaptive-reusebucket-')) and row['scans_per_revision']>=2:
            assert row['cached_records_current']==row['records']
            revisions=max(1,min(row['window'],row['rounds']))+int(row['pin_initial'])
            assert row['cached_records_distinct_revisions']==revisions*row['records']

def execute(task):
    binary,engine,records,changed,rounds,window,pin,mixed,scans=task
    command=[str(binary),'--engine',engine,'--records',str(records),'--changed',str(changed),'--rounds',str(rounds),'--window',str(window),'--pin-initial',str(pin).lower(),'--mixed',str(mixed).lower(),'--scans',str(scans)]
    wrapper=['/usr/bin/time','-l','-p']if platform.system()=='Darwin'else['/usr/bin/time','-v']
    result=study.run_bounded(wrapper+command,timeout=300)
    if result.returncode:raise RuntimeError(json.dumps(dict(command=command,code=result.returncode,stdout=result.stdout[-3000:],stderr=result.stderr[-3000:])))
    row=json.loads(result.stdout)
    for name,value in [('engine',engine),('records',records),('changed',changed),('rounds',rounds),('window',window),('pin_initial',pin),('mixed',mixed)]:assert row[name]==value,(name,row[name],value)
    assert row.get('scans_per_revision',0)==scans
    validate_row(row)
    if platform.system()=='Darwin':
        match=re.search(r'(\d+)\s+maximum resident set size',result.stderr)
        if match:row['instrumented_process_peak_rss']=int(match[1])
    else:
        match=re.search(r'Maximum resident set size \(kbytes\): (\d+)',result.stderr)
        if match:row['instrumented_process_peak_rss']=int(match[1])*1024
    return row

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary',type=pathlib.Path,required=True);parser.add_argument('--build-receipt',type=pathlib.Path,required=True);parser.add_argument('--label',required=True)
    parser.add_argument('--records',type=int,default=20000);parser.add_argument('--changed',type=int,default=200);parser.add_argument('--rounds',type=int,default=256);parser.add_argument('--windows',default='0,16,64');parser.add_argument('--workers',type=int,default=1);parser.add_argument('--engines')
    parser.add_argument('--scans',type=int,default=0,help='Read traversals after each revision; checks that no payloads are cloned')
    args=parser.parse_args();assert re.fullmatch(r'[A-Za-z0-9_.-]+',args.label)
    windows=[int(v)for v in args.windows.split(',')]
    if not (args.records>0 and 0<args.changed<=args.records and
            args.rounds*args.changed>=args.records and args.workers>0 and
            windows and all(w>=0 for w in windows) and args.scans>=0):
        parser.error('retention requires positive records/workers, 0 < changed <= records, '
                     'rounds * changed >= records and nonnegative windows')
    binary=args.binary.resolve();binding=json.loads(args.build_receipt.read_text())
    assert hashlib.sha256(binary.read_bytes()).hexdigest()==binding['binary_sha256']
    assert study.source_hash_at(binding['source_commit'])==binding['source_sha256']
    source=subprocess.check_output(['git','show',binding['source_commit']+':examples/retention_profile.rs'],cwd=ROOT)
    assert hashlib.sha256(source).hexdigest()==binding['example_sha256']
    folder=ROOT/'evidence'/args.label;folder.mkdir(exist_ok=False)
    engines=args.engines.split(',')if args.engines else ENGINES
    tasks=[(binary,e,args.records,args.changed,args.rounds,w,p,m,args.scans)for e,w,p,m in itertools.product(engines,windows,[False,True],[False,True])]
    random.Random(0xA11C).shuffle(tasks)
    layout=json.loads(subprocess.check_output([str(binary),'--layout-only'],text=True))
    receipt=dict(kind='instrumented-payload-retention',claim='Owned Vec payload bytes and live Value instances only, not total library heap; RSS includes instrumentation, metadata and fixture. No timing acceptance.',build=binding,build_receipt_sha256=hashlib.sha256(args.build_receipt.read_bytes()).hexdigest(),driver_sha256=hashlib.sha256(pathlib.Path(__file__).read_bytes()).hexdigest(),executor_sha256=hashlib.sha256((ROOT/'tools/study.py').read_bytes()).hexdigest(),driver_source_commit=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),layout=layout,engines=engines,records=args.records,changed=args.changed,rounds=args.rounds,windows=windows,workers=args.workers,tasks=len(tasks))
    receipt['scans_per_revision']=args.scans
    (folder/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n');completed=0;errors=[]
    with(folder/'runs.jsonl').open('w')as out,concurrent.futures.ThreadPoolExecutor(max_workers=args.workers)as pool:
        futures={pool.submit(execute,t):t for t in tasks}
        for future in concurrent.futures.as_completed(futures):
            try:row=future.result();out.write(json.dumps(row)+'\n');out.flush();completed+=1
            except Exception as error:errors.append(dict(task=list(map(str,futures[future])),error=str(error)));(folder/'errors.json').write_text(json.dumps(errors,indent=2)+'\n')
            if(completed+len(errors))%10==0:print(f'{completed} completed, {len(errors)} failures / {len(tasks)}',flush=True)
    receipt.update(completed=completed,errors=len(errors),complete=True);(folder/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n')
    print(json.dumps(dict(label=args.label,completed=completed,errors=len(errors))),flush=True);return int(bool(errors))
if __name__=='__main__':raise SystemExit(main())
