fn main() {
    let proto_dir = "../../contracts/proto";
    let proto = format!("{proto_dir}/qbank.proto");

    println!("cargo:rerun-if-changed={proto}");
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_SERVER");

    let server = std::env::var_os("CARGO_FEATURE_SERVER").is_some();

    tonic_build::configure()
        .build_server(server)
        .build_client(true)
        .compile_protos(&[proto], &[proto_dir])
        .expect("failed to compile qbank.proto");
}
