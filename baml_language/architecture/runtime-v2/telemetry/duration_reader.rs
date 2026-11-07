use std::collections::BTreeMap;
use prost::Message;
use btel_recorder::proto as p;
pub fn read(home:&std::path::Path)->serde_json::Value {
 let root=home.join("btel");let layout=btel_reader::layout::SourceLayout{recordings:root.join("recordings"),cas:root.join("cas"),root};
 let mut functions=BTreeMap::new();let mut paths=BTreeMap::new();let mut entries=Vec::new();let mut scales=std::collections::BTreeSet::new();
 for recording in btel_reader::discovery::discover(&layout).unwrap().recordings {
  assert_eq!(recording.partial_files,0);
  for file in recording.files {
   let r=p::RecordingFile::decode(std::fs::read(file.path).unwrap().as_slice()).unwrap();
   if let Some(d)=r.definitions{
    for f in d.functions {if let Some(p::function_definition::Resolution::Metadata(m))=f.resolution{functions.insert(f.function_id,serde_json::to_string(&(m.fqn,m.definition_key,m.owner_type_key,m.lambda_path)).unwrap());}}
    for path in d.call_paths{paths.insert(path.call_path_id,path);}
    for c in d.clock_epochs{assert_eq!(c.source,4,"ARM counter diagnostic requires an ARM clock domain");scales.insert((c.multiplier,c.shift));}
   }
   if let Some(a)=r.aggregates{entries.extend(a.entries);}
  }
 }
 assert_eq!(scales.len(),1,"duration domains must agree");let (mult,shift)=*scales.first().unwrap();
 let mut sums:BTreeMap<String,[u128;6]>=BTreeMap::new();
 for a in entries {
  let mut id=(a.node>>1) as u32;let mut path=Vec::new();
  while id!=0{let p=&paths[&id];path.push(serde_json::json!([p.visible_caller_function_id.map(|f|&functions[&f]),p.caller_pc,&functions[&p.callee_function_id],p.edge]));id=p.parent_call_path_id;}
  path.reverse(); // No aggregate exemptions: C5 covers collector task records only.
  let key=serde_json::to_string(&(path,a.node&1)).unwrap();let v=sums.entry(key).or_default();
  v[0]+=u128::from(a.count);v[1]+=u128::from(a.total_duration_ticks);v[2]+=u128::from(a.total_self_await_ticks);
  let o=a.outcomes.expect("exact outcomes");v[3]+=u128::from(o.errored);v[4]+=u128::from(o.cancelled);v[5]+=u128::from(o.panicked.expect("panic outcomes"));
 }
 let rows:Vec<_>=sums.into_iter().map(|(key,v)|serde_json::json!({"key":key,"count":v[0] as u64,"ticks":v[1] as u64,"ns":((v[1]*u128::from(mult))>>shift) as u64,"await":v[2] as u64,"errored":v[3] as u64,"cancelled":v[4] as u64,"panicked":v[5] as u64})).collect();serde_json::json!(rows)
}
