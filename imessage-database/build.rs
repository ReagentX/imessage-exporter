/*!
Build script for protobuf-generated modules.

Build modes:

- Default build (`cargo build -p imessage-database`):
  validates that generated protobuf Rust files are already present in the repository.
- Regeneration build (`cargo build -p imessage-database --features proto-regen`):
  regenerates protobuf Rust files from the `.proto` sources.

Useful commands:

- `cargo check -p imessage-database`
- `cargo check -p imessage-database --features proto-regen`
*/

#[cfg(not(feature = "proto-regen"))]
use std::fs::exists;

#[cfg(feature = "proto-regen")]
use std::{env, fs::copy, path::PathBuf};

#[cfg(feature = "proto-regen")]
const HANDWRITING_PROTO_INPUT: &str = "src/message_types/handwriting/handwriting.proto";
#[cfg(feature = "proto-regen")]
const HANDWRITING_GENERATED_NAME: &str = "handwriting.rs";
const HANDWRITING_OUTPUT: &str = "src/message_types/handwriting/handwriting_proto.rs";
#[cfg(feature = "proto-regen")]
const DIGITAL_TOUCH_PROTO_INPUT: &str = "src/message_types/digital_touch/digital_touch.proto";
#[cfg(feature = "proto-regen")]
const DIGITAL_TOUCH_GENERATED_NAME: &str = "digital_touch.rs";
const DIGITAL_TOUCH_OUTPUT: &str = "src/message_types/digital_touch/digital_touch_proto.rs";

fn main() {
    #[cfg(feature = "proto-regen")]
    {
        build_proto(
            HANDWRITING_PROTO_INPUT,
            HANDWRITING_GENERATED_NAME,
            HANDWRITING_OUTPUT,
        );
        build_proto(
            DIGITAL_TOUCH_PROTO_INPUT,
            DIGITAL_TOUCH_GENERATED_NAME,
            DIGITAL_TOUCH_OUTPUT,
        );
    }

    #[cfg(not(feature = "proto-regen"))]
    {
        ensure_generated(HANDWRITING_OUTPUT);
        ensure_generated(DIGITAL_TOUCH_OUTPUT);
    }
}

#[cfg(not(feature = "proto-regen"))]
fn ensure_generated(path: &str) {
    if !exists(path).unwrap_or(false) {
        panic!(
            "Missing generated protobuf module `{path}`. Re-generate with `cargo build -p imessage-database --features proto-regen`."
        );
    }
}

#[cfg(feature = "proto-regen")]
fn build_proto(input_proto: &str, generated_name: &str, output_rs: &str) {
    protobuf_codegen::Codegen::new()
        .pure()
        .input(input_proto)
        .include(".")
        .cargo_out_dir("p")
        .run_from_script();

    let mut generated = PathBuf::from(env::var("OUT_DIR").unwrap());
    generated.push("p");
    generated.push(generated_name);
    copy(generated, output_rs).unwrap();
}
