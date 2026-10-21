#!/usr/bin/env python3
"""Generic key/payload breadth; parallel selection, never a final holdout gate."""
import argparse,concurrent.futures,hashlib,itertools,json,pathlib,platform,random,subprocess,time
import study
ROOT=pathlib.Path(__file__).resolve().parents[1]
ENGINES=['hashbrown','hashbrown-sip','imbl-hash','imbl-fast','im-hash','im-fast','rpds-hash','rpds-fast','imhm','probe-flat','probe-paged16','probe-paged64','split','record-flat','record-paged16','record-paged64','record-paged256','shared-flat','shared-paged16','shared-paged64','shared-paged256','rill','rillfast']
ARRAYS=[f'array-{k}{p}'for k in ['copy','shared']for p in [16,64,256,1024]]+[f'array-{k}-low{p}'for k in ['copy','shared']for p in [16,256]]
ENGINES+=ARRAYS+['record-lowflat','probe-lowflat']
ARRAY_OWNED=[f'array-owned-{k}-low{p}'for k in ['copy','shared']for p in [16,256]]+[f'array-owned-{k}-sparse{p}'for k in ['copy','shared']for p in [64,1024]]
ARRAY_SPARSE=[f'array-{k}-sparse{p}'for k in ['copy','shared']for p in [64,1024]]
ENGINES+=ARRAY_OWNED+ARRAY_SPARSE
SHAPES=['u64-inline','string-inline','u64-owned256','string-owned256','u64-shared256','string-shared256','mixed-owned','mixed-shared']
CASES=['single','sparse','churn','burst','read','scan','slow','miss100']
def main():
 p=argparse.ArgumentParser();p.add_argument('mode',choices=['prove','thread','screen','serial']);p.add_argument('--binary',type=pathlib.Path,required=True);p.add_argument('--label',required=True);p.add_argument('--engines');p.add_argument('--shapes');p.add_argument('--cases');p.add_argument('--records',type=int,default=20000);p.add_argument('--readers',type=int,default=20000);p.add_argument('--rounds',type=int,default=16);p.add_argument('--repetitions',type=int,default=1);p.add_argument('--workers',type=int,default=3);p.add_argument('--seeds',default='4815162342,2937471921');a=p.parse_args()
 engines=a.engines.split(',')if a.engines else ENGINES;shapes=a.shapes.split(',')if a.shapes else SHAPES;cases=a.cases.split(',')if a.cases else CASES;seeds=[int(n)for n in a.seeds.split(',')];assert all(c.isalnum()or c in'-_.'for c in a.label)
 binary=a.binary.resolve();folder=ROOT/'evidence'/a.label;folder.mkdir(exist_ok=False,parents=True);workers=1 if a.mode=='serial'else a.workers
 tasks=[(binary,e,c,s,a.records,a.readers,a.rounds,a.mode in ('prove','thread'),shape,rep,a.mode=='thread')for rep in range(a.repetitions)for e,c,s,shape in itertools.product(engines,cases,seeds,shapes)];random.Random(0x5A9E).shuffle(tasks)
 receipt=dict(schema=1,layer='generic-shapes',mode=a.mode,claim='exact behavior'if a.mode in ('prove','thread')else'development selection only, not final acceptance',workers=workers,source_sha256=study.source_hash(),source_commit=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),driver_sha256=hashlib.sha256(pathlib.Path(__file__).read_bytes()).hexdigest(),executor_sha256=hashlib.sha256((ROOT/'tools/study.py').read_bytes()).hexdigest(),engines=engines,shapes=shapes,cases=cases,seeds=seeds,records=a.records,readers=a.readers,rounds=a.rounds,repetitions=a.repetitions,tasks=len(tasks),platform=platform.platform(),rustc=subprocess.check_output(['rustc','-Vv'],text=True),machine=platform.machine())
 receipt.update(study.build_binding(binary));receipt['driver_source_commit']=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip()
 (folder/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n');errors=[];completed=0;outputs={}
 with(folder/'runs.jsonl').open('w')as out,concurrent.futures.ThreadPoolExecutor(max_workers=workers)as pool:
  futures={pool.submit(study.execute,t):t for t in tasks}
  for future in concurrent.futures.as_completed(futures):
   try:
    row=future.result();out.write(json.dumps(row)+'\n');out.flush();completed+=1
    if a.mode not in ('prove','thread'):
     key=(row['shape'],row['case'],row['seed']);signature=(row['checksum'],row['read_checksum'],row['final_len']);prior=outputs.setdefault(key,signature)
     if prior!=signature:errors.append(dict(output_mismatch=key,engine=row['engine'],expected=prior,actual=signature))
   except Exception as e:errors.append(dict(task=list(map(str,futures[future])),error=str(e)))
   if errors:(folder/'errors.json').write_text(json.dumps(errors,indent=2)+'\n')
   if(completed+len(errors))%50==0:print(f'{completed} completed, {len(errors)} failures / {len(tasks)}',flush=True)
 receipt.update(completed=completed,errors=len(errors),output_agreement=not errors,finished_at_unix=time.time());(folder/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n');print(json.dumps(dict(label=a.label,completed=completed,errors=len(errors))),flush=True);return int(bool(errors))
if __name__=='__main__':raise SystemExit(main())
