#!/usr/bin/env python3
"""Register all adaptive policies and matched fixed controls before measurement."""
import itertools,json,pathlib
root=pathlib.Path(__file__).resolve().parents[1]
policies=[f'adaptive-{mode}-{h}-{t}-{d}-{g}-{p}-{r}'for mode,h,t,d,g,p,r in itertools.product(
 ['inline','shared','cache'],['sip','fast'],[1,10,100],[1,3],['any','drop'],[16,64],['oneway','reset'])]
references=['hashbrown','hashbrown-sip','std-fast','std-hash','indexmap-fast','imbl-fast','rpds-fast','record-paged16','record-paged64','inline-owned-index-25','vector-rb-fast','vector-cowvec-fast']
selected=[f'adaptive-{mode}-{h}-1-3-drop-{p}-{r}'for mode,h,p,r in itertools.product(['inline','shared','cache'],['sip','fast'],[16,64],['oneway','reset'])]
assert len(policies)==288 and len(selected)==24
plan={'schema':1,'status':'Registered adaptive layout development; no comparative admission','source_binary_version':'v30','seeds':[4815162342,2937471921],
 'broad_candidates':policies,'selected_candidates':selected,'scalar_references':references,'generic_references':references,
 'selection_basis':'All 144 layout/transition policies with standard/fast builders. Shared-cache-off controls isolate traversal caching; one-way controls isolate the generic half-density flat reset. Same original scalar/generic staging, revision, feed, trace, warmup and borrowed-value oracle harnesses. No case labels or trace lookahead enter the backend.',
 'proof':{'small':{'records':1000,'readers':1000,'rounds':8},'full_selected':{'records':20000,'readers':20000,'rounds':12},'thread_selected':{'records':20000,'readers':20000,'rounds':12,'cases':['sparse','churn']}},
 'timing':{'scalar_rounds':32,'generic_rounds':12,'serial_repetitions':3,'parallel_claim':'Development selection only; existing frozen proofs may overlap','serial_claim':'Descriptive development only; unchanged final corrected-confidence holdout gate still required'},
 'initial_phases':['proof-small','screen'],
 'warmup':'Original harness publishes eight small update batches and a bulk restore outside timing for every backend. Adaptive reset is driven only by submitted op count >= ceil(current_len/2), never a warmup flag. One-way controls retain irreversible behavior. Cold and repeated transition diagnostics remain required.',
 'remaining_required':['full-size behavior','four-reader behavior','serial comparison','cold conversion/cache/repeated-transition diagnostics','streaming retained-payload/cache reclamation','source admission closure','complete final holdout gate'],
 'build':{'command':'python3 tools/freeze_benchmarks.py v30 --bench-prefix adaptive_','bench_targets':['adaptive_state','adaptive_shapes'],'reason':'Dedicated entry points reuse exact original harness code without making unused legacy dispatch reachable.'},
 'expected_initial_processes_per_phase':47400}
p=root/'research/v30-development-plan.json';assert not p.exists();p.write_text(json.dumps(plan,indent=2)+'\n')
print(len(policies)+len(references),'engines;',plan['expected_initial_processes_per_phase'],'processes per initial phase')
