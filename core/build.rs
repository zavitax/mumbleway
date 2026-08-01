fn main() {
    let mut cfg = prost_build::Config::new();
    // Mumble.proto is proto2 and uses `bytes` fields heavily; map them to Bytes for
    // cheap slicing of tunnelled audio payloads.
    cfg.bytes(["."]);
    cfg.compile_protos(
        &["proto/Mumble.proto", "proto/MumbleUDP.proto"],
        &["proto/"],
    )
    .expect("failed to compile Mumble protobuf schemas");

    println!("cargo:rerun-if-changed=proto/Mumble.proto");
    println!("cargo:rerun-if-changed=proto/MumbleUDP.proto");
    println!("cargo:rerun-if-changed=build.rs");
}
