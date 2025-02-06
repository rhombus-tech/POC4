fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proto_file = std::fs::canonicalize("../../tee/proto/tee_service.proto")?;
    let proto_dir = proto_file.parent().unwrap();
    
    println!("cargo:rerun-if-changed={}", proto_file.display());
    
    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_well_known_types(true)
        .type_attribute(".", "#[derive(serde::Serialize, serde::Deserialize)]")
        .out_dir("src/proto") // Output generated code to src/proto
        .compile(&[proto_file.clone()], &[proto_dir])?;
    
    Ok(())
}
