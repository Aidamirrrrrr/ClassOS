fn main() {
    println!("cargo:rerun-if-changed=../../proto/local_ipc.proto");
    prost_build::compile_protos(&["../../proto/local_ipc.proto"], &["../../proto"])
        .expect("failed to compile local_ipc.proto: check protoc is installed and on PATH");
}
