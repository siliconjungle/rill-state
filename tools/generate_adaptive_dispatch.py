#!/usr/bin/env python3
"""Generate fixed adaptive policies over the unchanged shared benchmark harnesses."""
import itertools
import pathlib
import subprocess
import os
ROOT=pathlib.Path(__file__).resolve().parents[1]
for layer in ['state','shapes']:
 generic=layer=='shapes'
 kv='K,V'if generic else'Key,Value'
 params='K:Key,V:Value,'if generic else''
 generics='K,V,'if generic else''
 method_new='new'if generic else'empty'
 get='AdaptiveMap::get(self,k)'if generic else'AdaptiveMap::get(self,k).copied()'
 get_type='Option<&V>'if generic else'Option<Value>'
 key_type='K'if generic else'Key'
 val_type='V'if generic else'Value'
 visit_args='&K,&V'if generic else'Key,Value'
 visit_body='AdaptiveMap::visit(self,f);'if generic else'AdaptiveMap::visit(self,|k,v|f(*k,*v));'
 delete='k.clone()'if generic else'*k'
 imp=(f'impl<{params}S:BuildHasher+Clone+Default+Send+Sync,const T:usize,const D:usize,const B:bool,const P:usize,F:FlatMode<{kv},S>+Send+Sync,const R:bool> Map'+(f'<K,V>'if generic else'')+f' for AdaptiveMap<{kv},S,T,D,B,P,F,R> where F::Data:Send+Sync {{\n'
      f' fn {method_new}()->Self {{Self::default()}}\n fn get(&self,k:&{key_type})->{get_type} {{{get}}}\n'
      f' fn put(&mut self,k:{key_type},v:{val_type}) {{self.apply(&[Mutation::Put(k,v)]);}}\n'
      f' fn del(&mut self,k:&{key_type}) {{self.apply(&[Mutation::Delete({delete})]);}}\n'
      ' fn len(&self)->usize {AdaptiveMap::len(self)}\n'
      f' fn visit(&self,'+('f'if generic else'mut f')+f':impl FnMut({visit_args})) {{{visit_body}}}\n'
      f' fn batch(&mut self,ops:&[Mutation<{kv}>]) {{self.apply(ops);}}\n}}\n')
 name=f'AdaptiveMap<{kv},S,$t,$d,$b,$p,F,$r>'
 compare=f'Compared::<{generics}{name},NativeRoot<{name}>>::new()'
 run=f'run::<K,V,_>({compare},o)'if generic else f'run({compare},o)'
 body='//! Generated fixed policies. Reuses the parent state/revision/feed/trace/oracle harness.\nuse super::*;\nuse rill_state::experimental::{AdaptiveMap,FlatMode,InlineFlat,SharedFlat};\n'+imp
 body+=f'fn policy<{params}S:BuildHasher+Clone+Default+Send+Sync,F:FlatMode<{kv},S>+Send+Sync>(o:&Options,p:&[&str]) where F::Data:Send+Sync {{\n'
 body+=' let threshold=p[2].parse::<usize>().unwrap();let delay=p[3].parse::<usize>().unwrap();let gate=match p[4]{"any"=>false,"drop"=>true,_=>panic!("invalid gate")};let page=p[5].parse::<usize>().unwrap();let reset=match p[6]{"oneway"=>false,"reset"=>true,_=>panic!("invalid direction policy")};\n'
 body+=' macro_rules! call {($t:literal,$d:literal,$b:literal,$p:literal,$r:literal)=>{'+run+'};}\n match(threshold,delay,gate,page,reset){\n'
 for t,d,b,p,r in itertools.product([1,10,100],[1,3],[False,True],[16,64],[False,True]):
  flag=str(b).lower();body+=f' ({t},{d},{flag},{p},{str(r).lower()})=>call!({t},{d},{flag},{p},{str(r).lower()}),\n'
 body+=' _=>panic!("unregistered adaptive policy"),}\n}\n'
 body+=f'fn dispatch<{"K:Key,V:Value"if generic else""}>(o:&Options) {{\n'if generic else'fn dispatch(o:&Options) {\n'
 body+=' if let Some(tail)=o.engine.strip_prefix("adaptive-"){let p:Vec<_>=tail.split(\'-\').collect();assert_eq!(p.len(),7);match(p[0],p[1]){\n'
 for mode,typ in [('inline','InlineFlat'),('shared','SharedFlat<0>'),('cache','SharedFlat<2>')]:
  for h,s in [('sip','std::collections::hash_map::RandomState'),('fast','hashbrown::DefaultHashBuilder')]:
   body+=f' ("{mode}","{h}")=>policy::<{generics}{s},{typ}>(o,&p),\n'
 body+=' _=>panic!("unregistered flat mode/builder"),}return;}\n'
 control={
 'hashbrown':f'hashbrown::HashMap<{kv}>',
 'hashbrown-sip':f'hashbrown::HashMap<{kv},std::collections::hash_map::RandomState>',
 'std-fast':f'std::collections::HashMap<{kv},hashbrown::DefaultHashBuilder>',
 'std-hash':f'std::collections::HashMap<{kv}>',
 'indexmap-fast':f'indexmap::IndexMap<{kv},hashbrown::DefaultHashBuilder>',
 'imbl-fast':f'imbl::GenericHashMap<{kv},hashbrown::DefaultHashBuilder,imbl::shared_ptr::DefaultSharedPtr>',
 'rpds-fast':f'rpds::HashTrieMapSync<{kv},hashbrown::DefaultHashBuilder>',
 'record-paged16':f'rill_state::experimental::RecordMap<{kv},hashbrown::DefaultHashBuilder,70,16>',
 'record-paged64':f'rill_state::experimental::RecordMap<{kv},hashbrown::DefaultHashBuilder,70,64>',
 'inline-owned-index-25':f'rill_state::experimental::InlineSliceMap<{kv},hashbrown::DefaultHashBuilder,25,rill_state::experimental::OwnedHasher,false>',
 'vector-rb-fast':f'vector_map::VectorMap<{kv},hashbrown::DefaultHashBuilder,vector_map::Rb>',
 'vector-cowvec-fast':f'vector_map::VectorMap<{kv},hashbrown::DefaultHashBuilder,vector_map::CowVec>',
 }
 body+=' macro_rules! control {($m:ty)=>{'+(f'run::<K,V,_>(Compared::<K,V,$m>::new(),o)'if generic else'run(Compared::<$m>::new(),o)')+'};}\nmatch o.engine.as_str(){\n'
 for label,ty in control.items():body+=f' "{label}"=>control!({ty}),\n'
 body+=' _=>panic!("unregistered reference"),}\n}\n'
 body+='pub(super) fn entry() {let mut o=options();if !std::env::args().any(|a|a=="--engine"){o.engine="adaptive-inline-fast-1-3-drop-16-reset".into();}\n' 
 if generic:
  body+='match o.shape.as_str(){\n'
  for shape,k,v in [('u64-inline','u64','[u64;4]'),('string-inline','String','[u64;4]'),('u64-owned256','u64','Vec<u8>'),('string-owned256','String','Vec<u8>'),('u64-shared256','u64','Arc<[u8]>'),('string-shared256','String','Arc<[u8]>'),('mixed-owned','VariableKey','MixedOwned'),('mixed-shared','VariableKey','MixedShared')]:body+=f' "{shape}"=>dispatch::<{k},{v}>(&o),\n'
  body+=' _=>panic!("unknown shape"),}\n'
 else:body+='dispatch(&o);\n'
 body+='}\n'
 path=ROOT/f'benches/support/adaptive_{layer}.rs';path.write_text(body)

subprocess.run(["rustfmt","--edition","2021",*[str(ROOT/f"benches/support/adaptive_{name}.rs")for name in ["state","shapes"]]],env=dict(os.environ,RUSTUP_TOOLCHAIN="stable"),check=True)
