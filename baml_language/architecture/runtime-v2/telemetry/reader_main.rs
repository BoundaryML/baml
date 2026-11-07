mod telemetry;
mod known_telemetry_divergences;
use prost::Message;
fn main() {
    let args: Vec<_> = std::env::args_os().collect();
    if args[1] == "--clocks" {
        let home=std::path::Path::new(&args[2]);
        let root=home.join("btel");
        let layout=btel_reader::layout::SourceLayout { recordings:root.join("recordings"),cas:root.join("cas"),root };
        let mut clocks=std::collections::BTreeSet::new();
        for recording in btel_reader::discovery::discover(&layout).unwrap().recordings {
            assert_eq!(recording.partial_files,0);
            for file in recording.files {
                let decoded=btel_recorder::proto::RecordingFile::decode(std::fs::read(file.path).unwrap().as_slice()).unwrap();
                for epoch in decoded.definitions.into_iter().flat_map(|d| d.clock_epochs) {
                    clocks.insert((epoch.source,epoch.fallback));
                }
            }
        }
        println!("{}",serde_json::to_string(&clocks).unwrap());
        return;
    }
    let trace=telemetry::read(std::path::Path::new(&args[1]));
    std::fs::write(&args[2],&trace).unwrap();
    if let Some(path)=args.get(3) {
        std::fs::write(path,known_telemetry_divergences::without_gc(&trace,"v2")).unwrap();
    }
}
