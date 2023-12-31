fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::compile_protos("../crates/exporter/proto/exporter.proto")?;
    Ok(())
}
