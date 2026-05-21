//! Compile `proto/noedb/v1/sql.proto` via vendored `protoc` (no system install).

fn main() -> Result<(), Box<dyn std::error::Error>> {
    std::env::set_var("PROTOC", protoc_bin_vendored::protoc_bin_path()?);
    let proto = "../../proto/noedb/v1/sql.proto";
    println!("cargo:rerun-if-changed={proto}");
    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(&[proto], &["../../proto"])?;
    Ok(())
}
