// OUT_DIR=../../packages/proto/src/protos cargo run

fn main() {
    let includes = "../../proto";

    let ext = std::ffi::OsStr::new("proto");
    let protos: Vec<_> = walkdir::WalkDir::new(includes).into_iter()
        .map(|x| x.unwrap())
        .filter(|x| x.path().extension() == Some(ext))
        .map(|x| x.into_path())
        .collect();

    for x in &protos {
        println!("{}", x.display());
    }

    prost_build::compile_protos(&protos, &[includes]).unwrap();
}
