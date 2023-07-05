// OUT_DIR=../../packages/proto/src/protos cargo run

fn main() {
    let includes = "../../proto";
    let out_dir = "../../packages/proto/src/protos";

    let ext = std::ffi::OsStr::new("proto");
    let protos: Vec<_> = walkdir::WalkDir::new(includes)
        .into_iter()
        .map(|x| x.unwrap())
        .filter(|x| x.path().extension() == Some(ext))
        .map(|x| x.into_path())
        .collect();

    println!("[info ] Compiling {} ...", includes);

    // prost_build::compile_protos(&protos, &[includes]).unwrap();

    tonic_build::configure()
        .build_client(true)
        .compile_well_known_types(true)
        .client_mod_attribute(".", r#"#[cfg(feature = "client")]"#)
        .build_server(true)
        .server_mod_attribute(".", r#"#[cfg(feature = "server")]"#)
        .disable_comments("../../proto/google/protobuf/any.proto")
        .disable_comments("../../proto/google/api/http.proto")
        .out_dir(out_dir)
        .compile(&protos, &[includes]).unwrap();
}
