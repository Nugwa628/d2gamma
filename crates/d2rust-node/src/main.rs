//! `d2rust-node`: D2RUST's official game server ([`d2rust_node`]).

fn main() -> std::process::ExitCode {
    d2rust_node::main(d2rust_node::NodeGame::own())
}
