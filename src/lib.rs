

pub mod api {
    // Used to pull in generated Rust in target build folder from .proto
    tonic::include_proto!("api");
}

// Pull in modules defined in subdirs below
pub mod actor;
