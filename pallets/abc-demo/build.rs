fn main() {
    prost_build::compile_protos(&["actor-delegate.proto"], &["../../../tea-codec/proto"]).unwrap();
}
