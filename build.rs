fn main() {
    println!("cargo:rerun-if-changed=src/grpcpb/utcp.proto");
    let out_dir = std::path::Path::new("src/grpcpb/generated");
    if !out_dir.exists() {
        std::fs::create_dir_all(out_dir).expect("failed to create gRPC output directory");
    }
    tonic_build::configure()
        .build_server(true)
        .out_dir(out_dir)
        .compile(&["src/grpcpb/utcp.proto"], &["src/grpcpb"])
        .expect("Failed to compile gRPC protos");
}
