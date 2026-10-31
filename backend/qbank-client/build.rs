fn main() {
    let proto_dir = "../../contracts/proto";
    let proto = format!("{proto_dir}/qbank.proto");

    println!("cargo:rerun-if-changed={proto}");

    tonic_build::configure()
        .build_server(false)
        .compile_protos(&[proto], &[proto_dir])
        .expect("failed to compile qbank.proto");
}
