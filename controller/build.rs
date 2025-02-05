use std::env;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_dir = PathBuf::from(env::var("OUT_DIR")?);
    let proto_file = "../../tee/proto/tee_service.proto";
    let proto_dir = "../../tee/proto";

    println!("cargo:rerun-if-changed={}", proto_file);
    println!("cargo:rerun-if-changed={}", proto_dir);

    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .file_descriptor_set_path(out_dir.join("tee_service_descriptor.bin"))
        .out_dir(out_dir) // Let tonic-build use OUT_DIR
        .compile(
            &[proto_file],
            &[proto_dir],
        )?;
    Ok(())
}
