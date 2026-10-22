#!/usr/bin/env python3
"""Describe instrumented payload ownership; not a library heap or speed bound."""
import argparse,hashlib,itertools,json,pathlib
from retention_study import validate_row

def describe(folder):
    receipt=json.loads((folder/'receipt.json').read_text());assert receipt['complete']and receipt['errors']==0
    raw=(folder/'runs.jsonl').read_bytes();rows=[json.loads(line)for line in raw.splitlines()]
    expected=set(itertools.product(receipt['engines'],receipt['windows'],[False,True],[False,True]))
    actual=[(r['engine'],r['window'],r['pin_initial'],r['mixed'])for r in rows]
    assert len(rows)==receipt['tasks']==receipt['completed']==len(set(actual))and set(actual)==expected
    for row in rows:validate_row(row)
    baseline={(r['window'],r['pin_initial'],r['mixed']):r for r in rows if r['engine']=='hashbrown'}
    cells=[]
    for row in sorted(rows,key=lambda r:(r['mixed'],r['window'],r['pin_initial'],r['engine'])):
        other=baseline[(row['window'],row['pin_initial'],row['mixed'])]
        cells.append(dict(row,end_payload_ratio_to_hashbrown_copy=row['owned_payload_bytes_end']/other['owned_payload_bytes_end']))
    return dict(claim='Instrumented live payload Value instances and owned Vec lengths; excludes structural/allocator overhead. Process RSS includes instrumented fixture; not uninstrumented library heap or speed.',rows_sha256=hashlib.sha256(raw).hexdigest(),reporter_sha256=hashlib.sha256(pathlib.Path(__file__).read_bytes()).hexdigest(),layout=receipt['layout'],all_tracked_values_reclaimed=True,exact_journal_counts=True,cells=cells)

def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('folder',type=pathlib.Path);args=parser.parse_args()
    result=describe(args.folder);output=args.folder/'descriptive-memory.json'
    with output.open('x')as stream:json.dump(result,stream,indent=2);stream.write('\n')
    print(json.dumps(dict(output=str(output),cells=len(result['cells']))))
if __name__=='__main__':main()
