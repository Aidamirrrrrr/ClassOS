fn main() {
    println!("cargo:rerun-if-changed=../../proto/local_ipc.proto");
    println!("cargo:rerun-if-changed=../../proto/classos_network.proto");
    prost_build::compile_protos(
        &[
            "../../proto/local_ipc.proto",
            "../../proto/classos_network.proto",
        ],
        &["../../proto"],
    )
    .expect("не удалось скомпилировать proto-схемы: проверьте установку protoc и PATH");
}
