fn main() {
  let proto_file = "./proto/event.proto";

  tonic_prost_build::configure()
      .build_server(true)
      .compile_protos(&[proto_file], &["."])
      .unwrap_or_else(|e| panic!("protobuf compile error: {}", e));

  println!("cargo:rerun-if-changed={}", proto_file);
}
