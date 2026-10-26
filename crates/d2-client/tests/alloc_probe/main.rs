//! The allocator probe, as its own test binary so it can be built without
//! the shared-library Bevy (a global allocator and a second copy of std
//! in a shared library do not mix).

#[path = "../app_alloc_probe.rs"]
mod app_alloc_probe;
#[path = "../app_support/mod.rs"]
mod app_support;
