use std::{
    env,
    fs::{copy, exists},
    path::PathBuf,
};

fn main() {
    build_proto(
        "src/message_types/handwriting/handwriting.proto",
        "handwriting.rs",
        "src/message_types/handwriting/handwriting_proto.rs",
    );
    build_proto(
        "src/message_types/digital_touch/digital_touch.proto",
        "digital_touch.rs",
        "src/message_types/digital_touch/digital_touch_proto.rs",
    );
}

fn build_proto(input_proto: &str, generated_name: &str, output_rs: &str) {
    if !exists(output_rs).unwrap() {
        protobuf_codegen::Codegen::new()
            .pure()
            .input(input_proto)
            .include(".")
            .cargo_out_dir("p")
            .run_from_script();

        // Move generated file to correct location
        let mut generated = PathBuf::from(env::var("OUT_DIR").unwrap());
        generated.push("p");
        generated.push(generated_name);
        println!("{}", generated.to_str().unwrap());
        copy(generated, output_rs).unwrap();
    }
}
